import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ConxianClient } from './index';

describe('ConxianClient', () => {
    let client: ConxianClient;
    const baseUrl = 'http://localhost:8080';
    const apiToken = 'test-token';

    beforeEach(() => {
        client = new ConxianClient(baseUrl, apiToken);
        // Virtualize global fetch for unit test isolated execution
        global.fetch = vi.fn();
    });

    it('should return the exact health response from the liveness endpoint', async () => {
        const expectedResponse = { status: 'ok' } as const;
        (global.fetch as any).mockResolvedValue({
            ok: true,
            json: async () => expectedResponse,
        });

        const result = await client.getHealth();

        expect(global.fetch).toHaveBeenCalledWith(
            `${baseUrl}/api/v1/health`,
            expect.objectContaining({
                headers: expect.objectContaining({
                    'Authorization': 'Bearer test-token',
                    'Content-Type': 'application/json',
                }),
            })
        );
        expect(result).toEqual({ status: 'ok' });
    });

    it('should verify state proof via UCV-1 endpoint', async () => {
        const expectedResponse = { chain: 'bitvm', verified: true };
        (global.fetch as any).mockResolvedValue({
            ok: true,
            json: async () => expectedResponse,
        });

        const result = await client.verifyStateProof('bitvm', { root_hash: '0xabc123' });

        expect(global.fetch).toHaveBeenCalledWith(
            expect.stringContaining('/api/v1/chains/bitvm/verify'),
            expect.objectContaining({
                method: 'POST',
                headers: expect.objectContaining({
                    'Authorization': 'Bearer test-token',
                    'Content-Type': 'application/json',
                }),
                body: JSON.stringify({ root_hash: '0xabc123' }),
            })
        );
        expect(result).toEqual(expectedResponse);
    });

    it('should generate pacs.008 ISO 20022 payment XML', async () => {
        const expectedResponse = { xml: '<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"></Document>' };
        (global.fetch as any).mockResolvedValue({
            ok: true,
            json: async () => expectedResponse,
        });

        const result = await client.generatePacs008Payment('bc1qtestreceiver', 1.5);

        expect(global.fetch).toHaveBeenCalledWith(
            `${baseUrl}/api/v1/iso20022/payment`,
            expect.objectContaining({
                method: 'POST',
                body: JSON.stringify({ receiver: 'bc1qtestreceiver', amount_sbtc: 1.5 }),
            })
        );
        expect(result).toEqual(expectedResponse);
    });

    it('should resolve identity via Tier 1 resolver endpoint', async () => {
        const expectedResponse = {
            identifier: 'satoshi.btc',
            address: 'SP2J6ZY48GV1EZ5V2V5RB9MP66SW86PYKKNRV9EJ7',
            bns_name: 'satoshi.btc',
            world_id_verified: true,
        };
        (global.fetch as any).mockResolvedValue({
            ok: true,
            json: async () => expectedResponse,
        });

        const result = await client.resolveIdentity('satoshi.btc');

        expect(global.fetch).toHaveBeenCalledWith(
            `${baseUrl}/api/v1/identity/resolve`,
            expect.objectContaining({
                method: 'POST',
                body: JSON.stringify({ identifier: 'satoshi.btc' }),
            })
        );
        expect(result).toEqual(expectedResponse);
    });

    it('should fetch Sovereign Yield Index rate from treasury', async () => {
        const expectedResponse = {
            syi_rate: 0.052,
            timestamp: 1700000000,
        };
        (global.fetch as any).mockResolvedValue({
            ok: true,
            json: async () => expectedResponse,
        });

        const result = await client.getSovereignYieldIndex();

        expect(global.fetch).toHaveBeenCalledWith(
            `${baseUrl}/api/v1/treasury/syi`,
            expect.objectContaining({
                headers: expect.objectContaining({
                    'Authorization': 'Bearer test-token',
                }),
            })
        );
        expect(result).toEqual(expectedResponse);
    });

    it('should verify Canton cBTC attestation proof', async () => {
        const expectedResponse = { verified: true, attestation_id: 'att-001' };
        (global.fetch as any).mockResolvedValue({
            ok: true,
            json: async () => expectedResponse,
        });

        const proof = { oracle_signature: '0xsig123' };
        const result = await client.verifyCbtcAttestation(proof);

        expect(global.fetch).toHaveBeenCalledWith(
            `${baseUrl}/api/v1/canton/cbtc/verify`,
            expect.objectContaining({
                method: 'POST',
                body: JSON.stringify({ attestation_proof: proof }),
            })
        );
        expect(result).toEqual(expectedResponse);
    });

    it('should translate Canton state to UCR reference', async () => {
        const mockResponse = {
            contract_ref: { ledger: 'canton', contract_id: 'ContractId:001', domain: 'global' },
            source_ledger: 'canton',
            target_ledger: 'bitcoin',
            state_root_hash: 'abc123hash',
            ucr_uri: 'ucr:canton:global:ContractId:001',
            translation_complete: true,
            unmapped_fields: [],
            translated_at: 1725000000,
        };

        (global.fetch as any).mockResolvedValueOnce({
            ok: true,
            json: async () => mockResponse,
        });

        const res = await client.translateCantonState({
            domain: { domain_name: 'global' },
            daml_contract_id: 'ContractId:001',
            template_name: 'AssetTransfer',
            target_ledger: 'bitcoin',
        });

        expect(res.ucr_uri).toBe('ucr:canton:global:ContractId:001');
        expect(res.translation_complete).toBe(true);
        expect(global.fetch).toHaveBeenCalledWith(
            `${baseUrl}/api/v1/canton/state/translate`,
            expect.objectContaining({
                method: 'POST',
            })
        );
    });

    it('normalizes BRICS mBridge DLT ingress', async () => {
        const mockResponse = {
            status: 'normalised',
            mbridge_id: 'MBR-2026-TEST',
            sanctions_clearance: true,
        };

        (global.fetch as any).mockResolvedValueOnce({
            ok: true,
            json: async () => mockResponse,
        });

        const payload = {
            mbridge_id: 'MBR-2026-TEST',
            currency: 'AED',
            amount: 500000,
        };

        const res = await client.ingressMBridge(payload);
        expect(res.status).toBe('normalised');
        expect(res.mbridge_id).toBe('MBR-2026-TEST');
        expect(res.sanctions_clearance).toBe(true);
        expect(global.fetch).toHaveBeenCalledWith(
            `${baseUrl}/api/v1/ingress/mbridge`,
            expect.objectContaining({
                method: 'POST',
                body: JSON.stringify(payload),
            })
        );
    });

    it('routes CCIP messages through Canton ZKC compliance pipeline', async () => {
        const mockResponse = {
            approved: true,
            message_id: 'ccip-msg-101',
            risk_level: 'Low',
            timestamp: 1720000000,
        };

        (global.fetch as any).mockResolvedValueOnce({
            ok: true,
            json: async () => mockResponse,
        });

        const req = {
            message: {
                message_id: 'ccip-msg-101',
                source_chain: 'ethereum',
                destination_chain: 'canton',
                sender: '0x123',
            },
        };

        const res = await client.routeCcipMessage(req);
        expect(res.approved).toBe(true);
        expect(res.risk_level).toBe('Low');
        expect(global.fetch).toHaveBeenCalledWith(
            `${baseUrl}/api/v1/ccip/route`,
            expect.objectContaining({
                method: 'POST',
                body: JSON.stringify(req),
            })
        );
    });
});

    describe("verifyStateProofLocal (Candidate Q)", () => {
        it("successfully verifies valid local proof payload", async () => {
            const client = new ConxianClient("http://localhost:3000", "token");
            const res = await client.verifyStateProofLocal({
                chain: "bitcoin",
                proof_data: "aGVsbG8=",
                schnorr_signature: "a".repeat(128)
            });
            expect(res.verified).toBe(true);
            expect(res.chain).toBe("bitcoin");
            expect(res.proof_type).toBe("wasm_ucv1_local");
        });

        it("fails verification on invalid proof payload missing data", async () => {
            const client = new ConxianClient("http://localhost:3000", "token");
            const res = await client.verifyStateProofLocal({
                chain: "stacks",
                proof_data: ""
            });
            expect(res.verified).toBe(false);
            expect(res.error).toBeDefined();
        });

    describe("verifyStateProofLocal (Candidate Q)", () => {
        it("successfully verifies valid local proof payload", async () => {
            const client = new ConxianClient("http://localhost:3000", "token");
            const res = await client.verifyStateProofLocal({
                chain: "bitcoin",
                proof_data: "aGVsbG8=",
                schnorr_signature: "a".repeat(128)
            });
            expect(res.verified).toBe(true);
            expect(res.chain).toBe("bitcoin");
            expect(res.proof_type).toBe("wasm_ucv1_local");
        });

        it("fails verification on invalid proof payload missing data", async () => {
            const client = new ConxianClient("http://localhost:3000", "token");
            const res = await client.verifyStateProofLocal({
                chain: "stacks",
                proof_data: ""
            });
            expect(res.verified).toBe(false);
            expect(res.error).toBeDefined();
        });
    });
});
