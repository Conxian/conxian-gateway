import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  BABYLON_REHEARSAL_METADATA,
  createClient,
  DeveloperSandboxClient,
  requireApiToken,
  runProofPath,
} from './index';

const HEALTH_RESPONSE = {
  status: 'ok',
} as const;

describe('developer sandbox proof path', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('runs health, supported chains, and Babylon rehearsal validation through the SDK', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);

    fetchMock
      .mockResolvedValueOnce({
        ok: true,
        json: async () => HEALTH_RESPONSE,
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ supported_chains: ['babylon', 'bitvm'] }),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ chain: 'babylon', verified: true }),
      });

    const logs: unknown[][] = [];
    const result = await runProofPath(
      createClient({ CONXIAN_API_TOKEN: 'sandbox-token' }),
      (...values) => logs.push(values),
    );

    expect(fetchMock).toHaveBeenCalledTimes(3);
    const requestedUrls = fetchMock.mock.calls.map(([url]) => url);
    expect(requestedUrls).toEqual([
      'http://localhost:3000/api/v1/health',
      'http://localhost:3000/api/v1/chains/list',
      'http://localhost:3000/api/v1/chains/babylon/verify',
    ]);
    expect(requestedUrls).not.toContain('http://localhost:3000/api/v1/chains/bitvm/verify');

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
        body: JSON.stringify({
          type: 'finality_gadget',
          evidence: 'sandbox-rehearsal',
        }),
      }),
    );
    expect(result).toEqual({
      health: HEALTH_RESPONSE,
      supportedChains: { supported_chains: ['babylon', 'bitvm'] },
      babylonRehearsalValidation: { chain: 'babylon', verified: true },
    });
    expect(logs.map(([label]) => label)).toEqual([
      'health:',
      'supported chains:',
      'Babylon rehearsal validation:',
    ]);
  });

  it('allows the proof path to be injected with a client double', async () => {
    const client: DeveloperSandboxClient = {
      getHealth: vi.fn().mockResolvedValue(HEALTH_RESPONSE),
      getSupportedChains: vi.fn().mockResolvedValue({ supported_chains: ['babylon', 'bitvm'] }),
      verifyStateProof: vi.fn().mockResolvedValue({ chain: 'babylon', verified: true }),
    };

    await runProofPath(client, vi.fn());

    expect(client.getHealth).toHaveBeenCalledOnce();
    expect(client.getSupportedChains).toHaveBeenCalledOnce();
    expect(client.verifyStateProof).toHaveBeenCalledWith(
      'babylon',
      BABYLON_REHEARSAL_METADATA,
    );
  });

  it('fails clearly when the API token is missing or blank', () => {
    expect(() => requireApiToken({})).toThrow('CONXIAN_API_TOKEN is required');
    expect(() => requireApiToken({ CONXIAN_API_TOKEN: '   ' })).toThrow(
      'CONXIAN_API_TOKEN is required',
    );
  });
});
