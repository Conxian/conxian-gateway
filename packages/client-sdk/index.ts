/**
 * Conxian Client SDK
 * Institutional-grade helpers for bridging Bitcoin and Stacks state logic.
 */

import {
    ChainAdapterInfo,
    PreparedTransaction,
    StateProofVerificationResponse,
    DlcBond,
    MuSig2AggregatedKey,
    Pacs008PaymentResponse,
    IdentityResolutionResponse,
    SyiResponse,
    CbtcVerificationResponse
} from "@conxian/schemas";

export const GATEWAY_API_VERSION = "v1";

export interface HealthResponse {
    status: "ok";
}

export class ConxianClient {
    private baseUrl: string;
    private apiToken: string;

    constructor(baseUrl: string, apiToken: string) {
        this.baseUrl = baseUrl.replace(/\/$/, "");
        this.apiToken = apiToken;
    }

    private async request<T>(path: string, options: RequestInit = {}): Promise<T> {
        const url = `${this.baseUrl}/api/v1${path}`;
        const response = await fetch(url, {
            ...options,
            headers: {
                "Authorization": `Bearer ${this.apiToken}`,
                "Content-Type": "application/json",
                ...options.headers,
            },
        });

        if (!response.ok) {
            const error = await response.json().catch(() => ({ error: response.statusText }));
            throw new Error(`Conxian API Error: ${error.error || response.statusText}`);
        }

        return response.json();
    }

    async getSupportedChains(): Promise<ChainAdapterInfo> {
        return this.request<ChainAdapterInfo>("/chains/list");
    }

    async getChainHeight(chain: string): Promise<{ chain: string, height: number }> {
        return this.request<{ chain: string, height: number }>(`/chains/${chain}/height`);
    }

    async prepareTransaction(chain: string, details: any): Promise<PreparedTransaction> {
        return this.request<PreparedTransaction>(`/chains/${chain}/prepare`, {
            method: "POST",
            body: JSON.stringify(details),
        });
    }

    /**
     * UCV-1: Verify a state proof for a specific chain.
     */
    async verifyStateProof(chain: string, proofMetadata: any): Promise<StateProofVerificationResponse> {
        return this.request<StateProofVerificationResponse>(`/chains/${chain}/verify`, {
            method: "POST",
            body: JSON.stringify(proofMetadata),
        });
    }

    /**
     * CON-1269: DLC Bond creation (Sovereign Finance).
     */
    async createDlcBond(bond: DlcBond): Promise<{ bond_id: string }> {
        return this.request<{ bond_id: string }>("/dlc/bond", {
            method: "POST",
            body: JSON.stringify(bond),
        });
    }

    /**
     * CON-1270: MuSig2 Key Aggregation.
     */
    async aggregateMuSig2Keys(pubkeys: string[]): Promise<MuSig2AggregatedKey> {
        return this.request<MuSig2AggregatedKey>("/musig2/aggregate-keys", {
            method: "POST",
            body: JSON.stringify({ pubkeys }),
        });
    }

    /**
     * G-FI2: Generate ISO 20022 pacs.008 customer credit transfer payment XML.
     */
    async generatePacs008Payment(receiver: string, amountSbtc: number): Promise<Pacs008PaymentResponse> {
        return this.request<Pacs008PaymentResponse>("/iso20022/payment", {
            method: "POST",
            body: JSON.stringify({ receiver, amount_sbtc: amountSbtc }),
        });
    }

    /**
     * Tier 1 Identity Resolution (BNS, Web3.bio, World ID).
     */
    async resolveIdentity(identifier: string): Promise<IdentityResolutionResponse> {
        return this.request<IdentityResolutionResponse>("/identity/resolve", {
            method: "POST",
            body: JSON.stringify({ identifier }),
        });
    }

    /**
     * Fetch real-time Sovereign Yield Index (SYI) rate and market quotes.
     */
    async getSovereignYieldIndex(): Promise<SyiResponse> {
        return this.request<SyiResponse>("/treasury/syi");
    }

    /**
     * Canton Network cBTC non-custodial attestation verification.
     */
    async verifyCbtcAttestation(attestationProof: any): Promise<CbtcVerificationResponse> {
        return this.request<CbtcVerificationResponse>("/canton/cbtc/verify", {
            method: "POST",
            body: JSON.stringify({ attestation_proof: attestationProof }),
        });
    }

    async getHealth(): Promise<HealthResponse> {
        return this.request<HealthResponse>("/health");
    }
}
