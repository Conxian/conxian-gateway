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

/**
 * G-FI2: ISO 20022 pacs.008 Payment Initiation structures.
 */
export interface Pacs008PaymentRequest {
    receiver: string;
    amount_sbtc: number;
}

export interface Pacs008PaymentResponse {
    xml: string;
}

/**
 * Tier 1 Identity Resolution structures.
 */
export interface IdentityResolutionRequest {
    identifier: string;
}

export interface IdentityResolutionResponse {
    identifier: string;
    address?: string;
    bns_name?: string;
    world_id_verified?: boolean;
    web3_bio_profile?: any;
    error?: string;
}

/**
 * Sovereign Yield Index (SYI) Treasury structures.
 *
 * BTC/STX USD price quotes are not tracked by the treasury monitor, so they
 * are optional and omitted rather than synthesized.
 */
export interface SyiResponse {
    syi_rate: number;
    timestamp: number;
    btc_quote?: number;
    stx_quote?: number;
}

/**
 * Canton cBTC Non-Custodial Verification structures.
 */
export interface CbtcVerificationRequest {
    attestation_proof: any;
}

export interface CbtcVerificationResponse {
    verified: boolean;
    attestation_id?: string;
    error?: string;
}

/**
 * Canton State Translation structures (G-C4 / Candidate J).
 */
export interface CantonStateTranslationRequest {
    domain: {
        domain_name: string;
        synchronizer_endpoint?: string;
        public_observer?: boolean;
    };
    daml_contract_id: string;
    template_name?: string;
    payload_json?: string;
    target_ledger: string;
}

export interface UniversalContractRef {
    ledger: string;
    contract_id: string;
    domain?: string;
}

export interface CantonStateTranslationResponse {
    contract_ref: UniversalContractRef;
    source_ledger: string;
    target_ledger: string;
    state_root_hash?: string;
    ucr_uri?: string;
    translation_complete: boolean;
    unmapped_fields?: string[];
    translated_at: number;
}

/**
 * BRICS mBridge DLT Ingress structures (Candidate P / G-FI3).
 */
export interface MBridgeIngressPayload {
    mbridge_id: string;
    currency: string;
    amount: number;
    sender_cbdc_wallet?: string;
    receiver_cbdc_wallet?: string;
    consensus_signatures?: string[];
    dlt_state_proof?: string;
}

export interface MBridgeIngressResponse {
    status: string;
    mbridge_id: string;
    normalized_compliance_id?: string;
    sanctions_clearance?: boolean;
    error?: string;
}

/**
 * Chainlink CCIP Canton Connector Routing structures (G-C5).
 */
export interface CcipMessage {
    message_id: string;
    source_chain: string;
    destination_chain: string;
    sender: string;
    data?: string;
    token_amounts?: Array<{ token: string; amount: string }>;
}

export interface CcipRouteRequest {
    message: CcipMessage;
    elevated_scrutiny?: boolean;
}

export interface CcipRouteResponse {
    approved: boolean;
    message_id: string;
    risk_level: string;
    reason?: string;
    timestamp: number;
}
