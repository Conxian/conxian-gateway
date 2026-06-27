/**
 * CON-771: Shared domain schema for Governance Actions.
 */
export interface GovernanceAction {
  action_id: string;
  proposal_id: string;
  action_type: string; // e.g., "parameter_change", "treasury_allocation"
  payload: any;
  status: string;
  enacted_at?: number;
}

/**
 * CON-771: Shared domain schema for Treasury Events.
 */
export interface TreasuryEvent {
  event_id: string;
  asset: string;
  amount: string; // BigInt as string for JSON
  direction: "inflow" | "outflow";
  reason: string;
  timestamp: number;
  reference_id?: string;
}

/**
 * CON-771: Shared domain schema for Audit Events.
 */
export interface AuditEvent {
  event_id: string;
  domain: "release" | "governance" | "treasury" | "identity";
  actor: string;
  action: string;
  outcome: string;
  timestamp: number;
  metadata: any;
}

/**
 * CON-775: Release approval request.
 */
export interface ReleaseApprovalRequest {
  release_id: string;
  artifact_hash: string;
  environment: string;
  requester: string;
}

/**
 * CON-775: Admin action response.
 */
export interface AdminActionResponse {
  action_id: string;
  status: string;
  audit_event_id: string;
  message: string;
}

/**
 * UCV-1: Universal Chain Verification types.
 */
export interface StateProofVerificationRequest {
  chain: string;
  proof_metadata: any;
}

export interface StateProofVerificationResponse {
  chain: string;
  verified: boolean;
  error?: string;
}

/**
 * UCV-1: Chain Adapter Information.
 */
export interface ChainAdapterInfo {
    supported_chains: string[];
    trust_tiers?: Record<string, number>;
}

/**
 * UCV-1: Prepared transaction structure.
 */
export interface PreparedTransaction {
    chain: string;
    unsigned_tx: string;
    fee_estimate: string;
    metadata?: any;
}

/**
 * CON-1270: MuSig2 (BIP-327) primitives.
 */
export interface MuSig2AggregatedKey {
    aggregated_pubkey: string;
    participant_pubkeys: string[];
}

export interface MuSig2PartialSignature {
    participant_id: number;
    signature: string;
}

/**
 * CON-1269: DLC (Discreet Log Contracts) primitives.
 */
export interface DlcBond {
    bond_id: string;
    amount_btc: number;
    interest_rate: number;
    maturity_date: number;
    sovereign_alignment: boolean;
}
