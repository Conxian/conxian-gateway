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
});
