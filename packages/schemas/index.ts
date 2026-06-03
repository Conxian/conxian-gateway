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
