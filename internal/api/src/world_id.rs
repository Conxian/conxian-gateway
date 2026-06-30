use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::AppState;

/// World ID verification request — receives the proof from the frontend (IDKit/JS SDK)
#[derive(Debug, Deserialize)]
pub struct WorldIdVerifyRequest {
    pub nullifier_hash: String,
    pub proof: String,
    pub merkle_root: String,
    pub verification_level: String,
    pub action: String,
    pub signal: Option<String>,
}

/// World ID verification response
#[derive(Debug, Serialize)]
pub struct WorldIdVerifyResponse {
    pub success: bool,
    pub detail: String,
    pub nullifier_hash: String,
    pub credential_type: String,
    pub created_at: String,
}

/// Forward proof to World ID developer API for verification
pub async fn verify_world_id(
    State(_state): State<AppState>,
    Json(payload): Json<WorldIdVerifyRequest>,
) -> Result<Json<WorldIdVerifyResponse>, (StatusCode, String)> {
    let client = reqwest::Client::new();
    let app_id = std::env::var("WORLD_ID_APP_ID").unwrap_or_else(|_| "app_staging_".to_string());

    let verify_url = format!("https://developer.world.org/api/v4/verify/{}", app_id);

    let response = client
        .post(&verify_url)
        .json(&serde_json::json!({
            "nullifier_hash": payload.nullifier_hash,
            "proof": payload.proof,
            "merkle_root": payload.merkle_root,
            "verification_level": payload.verification_level,
            "action": payload.action,
            "signal": payload.signal.unwrap_or_default(),
        }))
        .send()
        .await
        .map_err(|e| {
            warn!(error = %e, "World ID API unreachable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("World ID API error: {e}"),
            )
        })?;

    let status = response.status();
    let body: serde_json::Value = response.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("World ID parse error: {e}"),
        )
    })?;

    if status.is_success() {
        info!(
            nullifier_hash = %payload.nullifier_hash,
            "World ID verification success"
        );
        Ok(Json(WorldIdVerifyResponse {
            success: true,
            detail: "Proof of human verified".to_string(),
            nullifier_hash: payload.nullifier_hash,
            credential_type: payload.verification_level,
            created_at: chrono_now(),
        }))
    } else {
        let detail = body
            .get("detail")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown error")
            .to_string();
        warn!(
            nullifier_hash = %payload.nullifier_hash,
            error = %detail,
            "World ID verification failed"
        );
        Err((
            StatusCode::BAD_REQUEST,
            format!("Verification failed: {detail}"),
        ))
    }
}

fn chrono_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}
