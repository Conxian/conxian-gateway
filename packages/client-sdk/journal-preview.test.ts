import { describe, expect, it, vi } from "vitest";
import { JOURNAL_PREVIEW_MAX_BYTES, JOURNAL_PREVIEW_MAX_LINES, JOURNAL_PREVIEW_VERSION,
    JournalPreviewError, prepareJournalPreview } from "./index";
import type { JournalPreviewRequest } from "./index";

const bitcoin = "bip122:000000000019d6689c085ae165831e93";
function request(): JournalPreviewRequest {
    return {
        schema_version: JOURNAL_PREVIEW_VERSION,
        organization_id: "synthetic-org", ledger_id: "synthetic-book", event_id: "test-event",
        accounting_date: "2026-10-16", currency: "ZAR", currency_decimals: 2,
        valuation_evidence_sha256: "2".repeat(64),
        source: {
            repository: "Conxian/conxian-nexus", revision: "a".repeat(40), network: bitcoin,
            asset: `${bitcoin}/slip44:0`, quantity_atomic: "4500000", asset_decimals: 8,
            transaction_id: "synthetic-transaction", observed_at: "2026-10-16T00:00:00.000Z",
            evidence_sha256: "1".repeat(64), settlement_status: "pending",
        },
        lines: [
            { account_id: "test-asset-account", side: "debit", amount_minor: "50000" },
            { account_id: "test-clearing-account", side: "credit", amount_minor: "50000" },
        ],
    };
}

async function rejects(value: unknown, code = "invalid_request") {
    await expect(prepareJournalPreview(value)).rejects.toMatchObject({ name: "JournalPreviewError", code });
}

describe("offline journal preview", () => {
    it("fails with a redacted typed error when runtime hashing is unavailable", async () => {
        vi.stubGlobal("crypto", undefined);
        try { await rejects(request(), "crypto_unavailable"); }
        finally { vi.unstubAllGlobals(); }
    });
    it("balances accounting currency separately from the source asset, without I/O or authority", async () => {
        const fetch = vi.fn(() => { throw new Error("Network must not be used"); });
        vi.stubGlobal("fetch", fetch);
        try {
            const r = request();
            const preview = await prepareJournalPreview(r);
            expect(preview.document).toEqual(r);
            expect(preview.totals).toEqual({ debit_minor: "50000", credit_minor: "50000", balanced: true });
            expect(preview.document.source.quantity_atomic).toBe("4500000");
            expect(preview.status).toBe("preview");
            expect(preview.source_evidence_verified).toBe(false);
            expect(preview.valuation_verified).toBe(false);
            expect(preview.posting_authorized).toBe(false);
            expect(preview).not.toHaveProperty("posted");
            expect(fetch).not.toHaveBeenCalled();
        } finally { vi.unstubAllGlobals(); }
    });
    it("uses exact arithmetic above the JavaScript integer limit and supports split lines", async () => {
        const r = request();
        r.lines = [
            { account_id: "a", side: "debit", amount_minor: "900719925474099312345" },
            { account_id: "b", side: "credit", amount_minor: "900719925474099312344" },
            { account_id: "c", side: "credit", amount_minor: "1" },
        ];
        expect((await prepareJournalPreview(r)).totals).toEqual({
            debit_minor: "900719925474099312345", credit_minor: "900719925474099312345", balanced: true,
        });
        r.lines[2].amount_minor = "2";
        await rejects(r, "unbalanced_journal");
    });
    it("keeps a stable scope key but changes the digest when an event's accounting payload changes", async () => {
        const r = request();
        const original = await prepareJournalPreview(r);
        r.lines.forEach(line => { line.amount_minor = "50001"; });
        const changed = await prepareJournalPreview(r);
        expect(changed.external_id).toBe(original.external_id);
        expect(changed.payload_sha256).not.toBe(original.payload_sha256);
        expect(original.external_id).toMatch(/^cj-[0-9a-f]{64}$/);
        expect(original.payload_sha256).toMatch(/^[0-9a-f]{64}$/);
    });
    it("binds every source, valuation and accounting field to the payload digest", async () => {
        const original = await prepareJournalPreview(request());
        for (const change of [
            (r: JournalPreviewRequest) => { r.accounting_date = "2026-10-17"; },
            (r: JournalPreviewRequest) => { r.currency = "USD"; },
            (r: JournalPreviewRequest) => { r.currency_decimals = 3; },
            (r: JournalPreviewRequest) => { r.valuation_evidence_sha256 = "3".repeat(64); },
            (r: JournalPreviewRequest) => { r.source.revision = "b".repeat(40); },
            (r: JournalPreviewRequest) => { r.source.quantity_atomic = "4500001"; },
            (r: JournalPreviewRequest) => { r.source.asset_decimals = 7; },
            (r: JournalPreviewRequest) => { r.source.transaction_id = "another-synthetic-transaction"; },
            (r: JournalPreviewRequest) => { r.source.observed_at = "2026-10-16T00:00:01.000Z"; },
            (r: JournalPreviewRequest) => { r.source.evidence_sha256 = "4".repeat(64); },
            (r: JournalPreviewRequest) => { r.source.settlement_status = "reorged"; },
            (r: JournalPreviewRequest) => { r.lines[0].account_id = "another-account"; },
            (r: JournalPreviewRequest) => { r.lines.reverse(); },
        ]) {
            const r = request(); change(r);
            const changed = await prepareJournalPreview(r);
            expect(changed.external_id).toBe(original.external_id);
            expect(changed.payload_sha256).not.toBe(original.payload_sha256);
        }
    });
    it("separates organization, ledger and event scopes without delimiter collisions", async () => {
        const original = await prepareJournalPreview(request());
        for (const field of ["organization_id", "ledger_id", "event_id"] as const) {
            const r = request(); r[field] += "-other";
            expect((await prepareJournalPreview(r)).external_id).not.toBe(original.external_id);
        }
        const a = { ...request(), organization_id: "a/b", ledger_id: "c" };
        const b = { ...request(), organization_id: "a", ledger_id: "b/c" };
        expect((await prepareJournalPreview(a)).external_id).not.toBe((await prepareJournalPreview(b)).external_id);
    });
    it("ignores object key order while preserving meaningful line order", async () => {
        const reverse = (value: unknown): unknown => {
            if (Array.isArray(value)) return value.map(reverse);
            if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).reverse().map(([k, v]) => [k, reverse(v)]));
            return value;
        };
        expect(await prepareJournalPreview(reverse(request()))).toEqual(await prepareJournalPreview(request()));
    });
    it("copies and freezes returned documents so later edits cannot invalidate their digest", async () => {
        const r = request();
        const p = await prepareJournalPreview(r);
        r.lines[0].amount_minor = "999";
        expect(p.document.lines[0].amount_minor).toBe("50000");
        expect(Reflect.set(p, "posting_authorized", true)).toBe(false);
        expect(Reflect.set(p.document.source, "settlement_status", "confirmed")).toBe(false);
        expect(Reflect.set(p.document.lines[0], "amount_minor", "999")).toBe(false);
        expect(Object.isFrozen(p.document.lines)).toBe(true);
    });
    for (const status of ["unknown", "pending", "confirmed", "reorged"] as const) {
        it(`never upgrades the submitted ${status} settlement claim to posting authority`, async () => {
            const r = request(); r.source.settlement_status = status;
            const p = await prepareJournalPreview(r);
            expect(p.source_evidence_verified).toBe(false);
            expect(p.posting_authorized).toBe(false);
        });
    }
    for (const value of ["0", "-1", "1.2", "01", "1e3", "1\n", "", 100, "1".repeat(79)]) {
        it(`rejects non-canonical or out-of-bounds amounts ${JSON.stringify(value).slice(0, 30)}`, async () => {
            const r = request();
            await rejects({ ...r, lines: [{ ...r.lines[0], amount_minor: value }, r.lines[1]] });
            await rejects({ ...r, source: { ...r.source, quantity_atomic: value } });
        });
    }
    for (const change of [
        { schema_version: "other" }, { posting_authorized: true }, { source_evidence_verified: true },
        { accounting_date: "2026-02-29" }, { accounting_date: "2026-13-01" }, { accounting_date: "2026-10-1" },
        { currency: "x402" }, { currency: "zar" }, { currency_decimals: -1 }, { currency_decimals: 1.5 }, { currency_decimals: 39 },
        { valuation_evidence_sha256: "not-evidence" }, { organization_id: " org" }, { ledger_id: "" }, { event_id: "x\n" },
        { lines: [] }, { lines: [request().lines[0]] }, { lines: Array(JOURNAL_PREVIEW_MAX_LINES + 1).fill(request().lines[0]) },
    ]) {
        it(`rejects incompatible journal fields ${JSON.stringify(change).slice(0, 60)}`, async () => { await rejects({ ...request(), ...change }); });
    }
    for (const change of [
        { network: "bitcoin" }, { network: "bip122:wrong" }, { asset: "X402" }, { asset: "eip155:1/slip44:60" },
        { revision: "dev" }, { evidence_sha256: "0x123" }, { observed_at: "2026-02-30T00:00:00.000Z" },
        { observed_at: "2026-10-16" }, { asset_decimals: 39 }, { settlement_status: "verified" }, { verified: true },
    ]) {
        it(`rejects incompatible source fields ${JSON.stringify(change).slice(0, 60)}`, async () => {
            const r = request(); await rejects({ ...r, source: { ...r.source, ...change } });
        });
    }
    it("accepts external assets using network-bound CAIP syntax, without claiming registered support", async () => {
        const r = request();
        r.source.network = "eip155:8453";
        r.source.asset = "eip155:8453/erc20:0x1111111111111111111111111111111111111111";
        r.source.asset_decimals = 6;
        expect((await prepareJournalPreview(r)).document.source.asset).toBe(r.source.asset);
    });
    it("accepts an actual leap day and exact scale boundaries", async () => {
        const r = request(); r.accounting_date = "2028-02-29"; r.currency_decimals = 0; r.source.asset_decimals = 38;
        expect((await prepareJournalPreview(r)).document.accounting_date).toBe("2028-02-29");
    });
    it("rejects wrong line shapes and hidden authority or currency fields", async () => {
        for (const line of [null, [], { account_id: "x", side: "debit", amount_minor: "1", currency: "USD" },
            { account_id: "x", side: "both", amount_minor: "1" }, { account_id: "", side: "debit", amount_minor: "1" }]) {
            const r = request(); await rejects({ ...r, lines: [line, r.lines[1]] });
        }
    });
    it("bounds bytes and rejects non-JSON values without echoing customer data", async () => {
        const cyclic: Record<string, unknown> = {}; cyclic.self = cyclic;
        for (const input of [cyclic, undefined, null, [], { ...request(), private: "synthetic-private".repeat(JOURNAL_PREVIEW_MAX_BYTES) },
            { ...request(), currency_decimals: Infinity }, { ...request(), event_id: 1n }, { ...request(), event_id: undefined }]) {
            await rejects(input);
            try { await prepareJournalPreview(input); }
            catch (error) { expect(error).toBeInstanceOf(JournalPreviewError); expect(String(error)).not.toContain("synthetic-private"); }
        }
    });
});
