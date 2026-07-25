import { ConxianClient, type HealthResponse } from '@conxian/client-sdk';

export const DEFAULT_GATEWAY_URL = 'http://localhost:3000';
export const VERIFICATION_CHAIN = 'babylon';
export const BABYLON_REHEARSAL_METADATA = {
  type: 'finality_gadget',
  evidence: 'sandbox-rehearsal',
} as const;

export interface DeveloperSandboxClient {
  getHealth(): Promise<HealthResponse>;
  getSupportedChains(): Promise<unknown>;
  verifyStateProof(chain: string, metadata: Record<string, unknown>): Promise<unknown>;
}

export interface ProofPathResult {
  health: HealthResponse;
  supportedChains: unknown;
  babylonRehearsalValidation: unknown;
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

  const babylonRehearsalValidation = await client.verifyStateProof(
    VERIFICATION_CHAIN,
    BABYLON_REHEARSAL_METADATA,
  );
  log('Babylon rehearsal validation:', JSON.stringify(babylonRehearsalValidation));

  return { health, supportedChains, babylonRehearsalValidation };
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
