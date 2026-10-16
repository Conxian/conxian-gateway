/** Offline accounting input validation. No evidence verification or ERP posting. */
export const JOURNAL_PREVIEW_VERSION = "conxian-journal-preview/1" as const;
export const JOURNAL_PREVIEW_MAX_BYTES = 65_536;
export const JOURNAL_PREVIEW_MAX_LINES = 256;

export interface JournalSourceClaim {
    repository: string;
    revision: string;
    network: string;
    asset: string;
    quantity_atomic: string;
    asset_decimals: number;
    transaction_id: string;
    observed_at: string;
    evidence_sha256: string;
    settlement_status: "unknown" | "pending" | "confirmed" | "reorged";
}

export interface JournalLine {
    account_id: string;
    side: "debit" | "credit";
    amount_minor: string;
}

export interface JournalPreviewRequest {
    schema_version: typeof JOURNAL_PREVIEW_VERSION;
    organization_id: string;
    ledger_id: string;
    event_id: string;
    accounting_date: string;
    currency: string;
    currency_decimals: number;
    valuation_evidence_sha256: string;
    source: JournalSourceClaim;
    lines: JournalLine[];
}

type Immutable<T> = T extends object ? { readonly [K in keyof T]: Immutable<T[K]> } : T;

export interface JournalPreview {
    readonly schema_version: typeof JOURNAL_PREVIEW_VERSION;
    readonly status: "preview";
    /** Stable within organization/ledger/event scope; not durable replay storage. */
    readonly external_id: string;
    /** Changes when any normalized document field or line order changes. */
    readonly payload_sha256: string;
    readonly document: Immutable<JournalPreviewRequest>;
    readonly totals: Readonly<{ debit_minor: string; credit_minor: string; balanced: true }>;
    readonly source_evidence_verified: false;
    readonly valuation_verified: false;
    readonly posting_authorized: false;
}

export class JournalPreviewError extends Error {
    constructor(readonly code: "invalid_request" | "unbalanced_journal" | "crypto_unavailable") {
        super(code === "unbalanced_journal" ? "Journal debits and credits must balance." :
            code === "crypto_unavailable" ? "SHA-256 is unavailable in this runtime." : "Invalid journal preview request.");
        this.name = "JournalPreviewError";
    }
}

function invalid(): never { throw new JournalPreviewError("invalid_request"); }

function object(value: unknown, keys: readonly string[]): Record<string, unknown> {
    if (!value || typeof value !== "object" || Array.isArray(value)) return invalid();
    const record = value as Record<string, unknown>;
    if (Object.keys(record).length !== keys.length || keys.some(key => !Object.hasOwn(record, key)) ||
        Object.keys(record).some(key => !keys.includes(key))) return invalid();
    return record;
}

function text(value: unknown, max = 128): string {
    if (typeof value !== "string" || value.length === 0 || value.length > max || value.trim() !== value ||
        /[\u0000-\u001f\u007f]/u.test(value)) return invalid();
    return value;
}

function match(value: unknown, regex: RegExp, max = 128): string {
    const result = text(value, max);
    if (!regex.test(result)) return invalid();
    return result;
}

function scale(value: unknown): number {
    if (typeof value !== "number" || !Number.isInteger(value) || value < 0 || value > 38) return invalid();
    return value === 0 ? 0 : value;
}

function amount(value: unknown): string {
    return match(value, /^[1-9][0-9]*$/u, 78);
}

function date(value: unknown): string {
    const result = match(value, /^[0-9]{4}-[0-9]{2}-[0-9]{2}$/u, 10);
    const time = Date.parse(`${result}T00:00:00.000Z`);
    if (!Number.isFinite(time) || new Date(time).toISOString().slice(0, 10) !== result) return invalid();
    return result;
}

function timestamp(value: unknown): string {
    const result = match(value, /^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3}Z$/u, 24);
    const time = Date.parse(result);
    if (!Number.isFinite(time) || new Date(time).toISOString() !== result) return invalid();
    return result;
}

function jsonInput(input: unknown): unknown {
    try {
        const json = JSON.stringify(input, (_key, value: unknown) => {
            if (value === undefined || typeof value === "bigint" || typeof value === "function" || typeof value === "symbol" ||
                (typeof value === "number" && !Number.isFinite(value))) return invalid();
            return value;
        });
        if (!json || json.length > JOURNAL_PREVIEW_MAX_BYTES || new TextEncoder().encode(json).length > JOURNAL_PREVIEW_MAX_BYTES) return invalid();
        return JSON.parse(json) as unknown;
    } catch { return invalid(); }
}

function normalize(input: unknown): JournalPreviewRequest {
    const r = object(jsonInput(input), ["schema_version", "organization_id", "ledger_id", "event_id", "accounting_date",
        "currency", "currency_decimals", "valuation_evidence_sha256", "source", "lines"]);
    if (r.schema_version !== JOURNAL_PREVIEW_VERSION) return invalid();
    const s = object(r.source, ["repository", "revision", "network", "asset", "quantity_atomic", "asset_decimals",
        "transaction_id", "observed_at", "evidence_sha256", "settlement_status"]);
    const network = match(s.network, /^[-a-z0-9]{3,8}:[-_a-zA-Z0-9]{1,32}$/u, 41);
    const asset = match(s.asset, /^[-a-z0-9]{3,8}:[-_a-zA-Z0-9]{1,32}\/[-a-z0-9]{3,8}:[-.%a-zA-Z0-9]{1,128}(\/[-.%a-zA-Z0-9]{1,78})?$/u, 258);
    if (asset.split("/")[0] !== network) return invalid();
    const settlementStatus = s.settlement_status;
    if (settlementStatus !== "unknown" && settlementStatus !== "pending" && settlementStatus !== "confirmed" && settlementStatus !== "reorged") return invalid();
    if (!Array.isArray(r.lines) || r.lines.length < 2 || r.lines.length > JOURNAL_PREVIEW_MAX_LINES) return invalid();
    const lines = r.lines.map((value): JournalLine => {
        const line = object(value, ["account_id", "side", "amount_minor"]);
        if (line.side !== "debit" && line.side !== "credit") return invalid();
        return { account_id: text(line.account_id), side: line.side, amount_minor: amount(line.amount_minor) };
    });
    // Construct in fixed field order: caller object key order cannot change a digest.
    return {
        schema_version: JOURNAL_PREVIEW_VERSION,
        organization_id: text(r.organization_id), ledger_id: text(r.ledger_id), event_id: text(r.event_id),
        accounting_date: date(r.accounting_date), currency: match(r.currency, /^[A-Z]{3}$/u, 3),
        currency_decimals: scale(r.currency_decimals),
        valuation_evidence_sha256: match(r.valuation_evidence_sha256, /^[0-9a-f]{64}$/u, 64),
        source: {
            repository: match(s.repository, /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u, 200),
            revision: match(s.revision, /^[0-9a-f]{40}$/u, 40), network, asset,
            quantity_atomic: amount(s.quantity_atomic), asset_decimals: scale(s.asset_decimals),
            transaction_id: text(s.transaction_id, 256), observed_at: timestamp(s.observed_at),
            evidence_sha256: match(s.evidence_sha256, /^[0-9a-f]{64}$/u, 64), settlement_status: settlementStatus,
        },
        lines,
    };
}

async function sha256(value: string): Promise<string> {
    try {
        const hash = await globalThis.crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
        return Array.from(new Uint8Array(hash), byte => byte.toString(16).padStart(2, "0")).join("");
    } catch { throw new JournalPreviewError("crypto_unavailable"); }
}

function freeze<T>(value: T): Immutable<T> {
    if (value && typeof value === "object") {
        for (const child of Object.values(value)) freeze(child);
        Object.freeze(value);
    }
    return value as Immutable<T>;
}

/**
 * Prepare an immutable, balanced accounting preview entirely offline.
 * All source, proof, finality, account, currency and valuation data are caller claims.
 * A future authorized outbox must persist scope/digest pairs and reject conflicts.
 */
export async function prepareJournalPreview(input: unknown): Promise<JournalPreview> {
    const document = normalize(input);
    let debit = 0n;
    let credit = 0n;
    for (const line of document.lines) {
        if (line.side === "debit") debit += BigInt(line.amount_minor);
        else credit += BigInt(line.amount_minor);
    }
    if (debit !== credit) throw new JournalPreviewError("unbalanced_journal");
    const externalId = `cj-${await sha256(JSON.stringify([JOURNAL_PREVIEW_VERSION, document.organization_id, document.ledger_id, document.event_id]))}`;
    const payloadSha256 = await sha256(JSON.stringify(document));
    return freeze<JournalPreview>({
        schema_version: JOURNAL_PREVIEW_VERSION, status: "preview", external_id: externalId,
        payload_sha256: payloadSha256, document,
        totals: { debit_minor: debit.toString(), credit_minor: credit.toString(), balanced: true },
        source_evidence_verified: false, valuation_verified: false, posting_authorized: false,
    });
}
