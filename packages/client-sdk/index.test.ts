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

    it('should verify state proof via UCV-1 endpoint', async () => {
        const mockResponse = { chain: 'bitcoin', verified: true };
        (global.fetch as any).mockResolvedValue({
            ok: true,
            json: async () => mockResponse,
        });

        const result = await client.verifyStateProof('bitcoin', { txid: '123' });

        expect(global.fetch).toHaveBeenCalledWith(
            expect.stringContaining('/api/v1/chains/bitcoin/verify'),
            expect.objectContaining({
                method: 'POST',
                headers: expect.objectContaining({
                    'Authorization': 'Bearer test-token',
                    'Content-Type': 'application/json',
                }),
                body: JSON.stringify({ txid: '123' }),
            })
        );
        expect(result).toEqual(mockResponse);
    });
});
