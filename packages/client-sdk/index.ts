/**
 * Conxian Client SDK
 * Institutional-grade helpers for bridging Bitcoin and Stacks state logic.
 */

import {
    ChainAdapterInfo,
    PreparedTransaction,
    StateProofVerificationResponse
} from "@conxian/schemas";

export const GATEWAY_API_VERSION = "v1";

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

    async getHealth(): Promise<any> {
        return this.request("/health");
    }
}
