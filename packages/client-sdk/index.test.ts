import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ConxianClient } from './index';

describe('ConxianClient', () => {
    let client: ConxianClient;
    const baseUrl = 'http://localhost:8080';
    const apiToken = 'test-token';

    beforeEach(() => {
        client = new ConxianClient(baseUrl, apiToken);
        // Mock global fetch
        global.fetch = vi.fn();
    });

    it('should return the exact health response from the liveness endpoint', async () => {
        const mockResponse = { status: 'ok' } as const;
        (global.fetch as any).mockResolvedValue({
            ok: true,
            json: async () => mockResponse,
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
        const mockResponse = { chain: 'bitvm', verified: true };
        (global.fetch as any).mockResolvedValue({
            ok: true,
            json: async () => mockResponse,
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
        expect(result).toEqual(mockResponse);
    });
});
