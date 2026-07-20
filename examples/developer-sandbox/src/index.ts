import { ConxianClient } from '@conxian/client-sdk';

export const DEFAULT_GATEWAY_URL = 'http://localhost:3000';
export const PROOF_CHAIN = 'bitvm';
export const PROOF_METADATA = { root_hash: '0xabc123' } as const;

export interface DeveloperSandboxClient {
  getHealth(): Promise<unknown>;
  getSupportedChains(): Promise<unknown>;
  verifyStateProof(chain: string, metadata: Record<string, unknown>): Promise<unknown>;
}

export interface ProofPathResult {
  health: unknown;
  supportedChains: unknown;
  bitvmProofRehearsal: unknown;
}

type Logger = (...values: unknown[]) => void;

export function requireApiToken(env: Record<string, string | undefined> = process.env): string {
  const token = env.CONXIAN_API_TOKEN?.trim();

  if (!token) {
    throw new Error(
      'CONXIAN_API_TOKEN is required. Provide a token for the Gateway instance you are using.',
    );
  }

  return token;
}

export function createClient(
  env: Record<string, string | undefined> = process.env,
): ConxianClient {
  const gatewayUrl = env.CONXIAN_GATEWAY_URL?.trim() || DEFAULT_GATEWAY_URL;
  return new ConxianClient(gatewayUrl, requireApiToken(env));
}

export async function runProofPath(
  client: DeveloperSandboxClient,
  log: Logger = console.log,
): Promise<ProofPathResult> {
  const health = await client.getHealth();
  log('health:', JSON.stringify(health));

  const supportedChains = await client.getSupportedChains();
  log('supported chains:', JSON.stringify(supportedChains));

  const bitvmProofRehearsal = await client.verifyStateProof(PROOF_CHAIN, PROOF_METADATA);
  log('bitvm proof rehearsal:', JSON.stringify(bitvmProofRehearsal));

  return { health, supportedChains, bitvmProofRehearsal };
}

export async function main(): Promise<void> {
  const gatewayUrl = process.env.CONXIAN_GATEWAY_URL?.trim() || DEFAULT_GATEWAY_URL;
  console.log(`gateway: ${gatewayUrl}`);
  await runProofPath(createClient());
}

if (require.main === module) {
  main().catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
