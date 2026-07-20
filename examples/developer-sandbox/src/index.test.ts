import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  createClient,
  DeveloperSandboxClient,
  PROOF_METADATA,
  requireApiToken,
  runProofPath,
} from './index';

describe('developer sandbox proof path', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('runs health, supported chains, and BitVM rehearsal through the live SDK surface', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);

    fetchMock
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ status: 'healthy' }),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ supported_chains: ['bitvm'] }),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ chain: 'bitvm', verified: true }),
      });

    const logs: unknown[][] = [];
    const result = await runProofPath(
      createClient({ CONXIAN_API_TOKEN: 'sandbox-token' }),
      (...values) => logs.push(values),
    );

    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      'http://localhost:3000/api/v1/health',
      'http://localhost:3000/api/v1/chains/list',
      'http://localhost:3000/api/v1/chains/bitvm/verify',
    ]);

    for (const [, request] of fetchMock.mock.calls) {
      expect(request.headers).toEqual(
        expect.objectContaining({
          Authorization: 'Bearer sandbox-token',
          'Content-Type': 'application/json',
        }),
      );
    }

    expect(fetchMock.mock.calls[2][1]).toEqual(
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify(PROOF_METADATA),
      }),
    );
    expect(result).toEqual({
      health: { status: 'healthy' },
      supportedChains: { supported_chains: ['bitvm'] },
      bitvmProofRehearsal: { chain: 'bitvm', verified: true },
    });
    expect(logs.map(([label]) => label)).toEqual([
      'health:',
      'supported chains:',
      'bitvm proof rehearsal:',
    ]);
  });

  it('allows the proof path to be injected with a client double', async () => {
    const client: DeveloperSandboxClient = {
      getHealth: vi.fn().mockResolvedValue({ status: 'healthy' }),
      getSupportedChains: vi.fn().mockResolvedValue({ supported_chains: ['bitvm'] }),
      verifyStateProof: vi.fn().mockResolvedValue({ chain: 'bitvm', verified: true }),
    };

    await runProofPath(client, vi.fn());

    expect(client.getHealth).toHaveBeenCalledOnce();
    expect(client.getSupportedChains).toHaveBeenCalledOnce();
    expect(client.verifyStateProof).toHaveBeenCalledWith('bitvm', PROOF_METADATA);
  });

  it('fails clearly when the API token is missing or blank', () => {
    expect(() => requireApiToken({})).toThrow('CONXIAN_API_TOKEN is required');
    expect(() => requireApiToken({ CONXIAN_API_TOKEN: '   ' })).toThrow(
      'CONXIAN_API_TOKEN is required',
    );
  });
});
