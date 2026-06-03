use crate::AppState;
use axum::{extract::State, http::StatusCode, Json};
use conxian_core::{
    AdminActionResponse, GovernanceDecisionRequest, ReleaseApprovalRequest, ReleaseDecisionRequest,
};
use serde_json::Value;
use tracing::info;

pub async fn request_release_approval(
    State(_state): State<AppState>,
    Json(payload): Json<ReleaseApprovalRequest>,
) -> Result<Json<AdminActionResponse>, (StatusCode, Json<Value>)> {
    info!(
        release_id = %payload.release_id,
        requester = %payload.requester,
        "Admin: Release approval requested"
    );

    Ok(Json(AdminActionResponse {
        action_id: format!("req-{}", uuid::Uuid::new_v4()),
        status: "pending".to_string(),
        audit_event_id: format!("audit-{}", uuid::Uuid::new_v4()),
        message: "Release approval request submitted to BOS control-plane".to_string(),
    }))
}

pub async fn submit_release_decision(
    State(_state): State<AppState>,
    Json(payload): Json<ReleaseDecisionRequest>,
) -> Result<Json<AdminActionResponse>, (StatusCode, Json<Value>)> {
    info!(
        release_id = %payload.release_id,
        decision = %payload.decision,
        approver = %payload.approver,
        "Admin: Release decision submitted"
    );

    Ok(Json(AdminActionResponse {
        action_id: format!("dec-{}", uuid::Uuid::new_v4()),
        status: payload.decision,
        audit_event_id: format!("audit-{}", uuid::Uuid::new_v4()),
        message: "Release decision recorded in BOS control-plane".to_string(),
    }))
}

pub async fn submit_governance_decision(
    State(_state): State<AppState>,
    Json(payload): Json<GovernanceDecisionRequest>,
) -> Result<Json<AdminActionResponse>, (StatusCode, Json<Value>)> {
    info!(
        proposal_id = %payload.proposal_id,
        decision = %payload.decision,
        voter = %payload.voter,
        "Admin: Governance decision submitted"
    );

    Ok(Json(AdminActionResponse {
        action_id: format!("gov-{}", uuid::Uuid::new_v4()),
        status: payload.decision,
        audit_event_id: format!("audit-{}", uuid::Uuid::new_v4()),
        message: "Governance decision recorded in BOS control-plane".to_string(),
    }))
}
