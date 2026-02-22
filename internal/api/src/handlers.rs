use axum::{extract::State, http::StatusCode, Json};
use compliance::ZkcVerifier;
use conxian_core::{AttestationRequest, SharedState};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "service": "conxian-gateway",
        "version": conxian_core::VERSION
    }))
}

pub async fn get_state(State(state): State<SharedState>) -> Json<Value> {
    {
        let mut s = state.write().unwrap();
        s.metrics.total_requests += 1;
    }
    let s = state.read().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let uptime = now.saturating_sub(s.start_time);

    Json(json!({
        "bitcoin": s.bitcoin,
        "stacks": s.stacks,
        "metrics": s.metrics,
        "start_time": s.start_time,
        "uptime_seconds": uptime
    }))
}

pub async fn get_metrics(State(state): State<SharedState>) -> String {
    let s = state.read().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let uptime = now.saturating_sub(s.start_time);

    format!(
        "# HELP gateway_total_requests The total number of API requests received.\n# TYPE gateway_total_requests counter\ngateway_total_requests {}\n# HELP gateway_verification_count The total number of attestation verifications attempted.\n# TYPE gateway_verification_count counter\ngateway_verification_count {}\n# HELP bitcoin_block_height The current block height of the Bitcoin chain.\n# TYPE bitcoin_block_height gauge\nbitcoin_block_height {}\n# HELP stacks_block_height The current block height of the Stacks chain.\n# TYPE stacks_block_height gauge\nstacks_block_height {}\n# HELP gateway_uptime_seconds The total uptime of the gateway in seconds.\n# TYPE gateway_uptime_seconds counter\ngateway_uptime_seconds {}\n",
        s.metrics.total_requests,
        s.metrics.verification_count,
        s.bitcoin.height,
        s.stacks.height,
        uptime
    )
}

pub async fn verify_attestation(
    State(state): State<SharedState>,
    Json(request): Json<AttestationRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    {
        let mut s = state.write().unwrap();
        s.metrics.total_requests += 1;
        s.metrics.verification_count += 1;
    }

    let verifier = ZkcVerifier::new();
    let result = match request {
        AttestationRequest::Ecdsa(a) => verifier.verify(&a),
        AttestationRequest::Schnorr(a) => verifier.verify_schnorr(&a),
    };

    match result {
        Ok(valid) => Ok(Json(json!({ "valid": valid }))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conxian_core::GatewayState;
    use std::sync::{Arc, RwLock};

    #[tokio::test]
    async fn test_health_check_handler() {
        let res = health_check().await;
        assert_eq!(res.0["status"], "healthy");
        assert_eq!(res.0["version"], conxian_core::VERSION);
    }

    #[tokio::test]
    async fn test_get_state_handler() {
        let state = Arc::new(RwLock::new(GatewayState::default()));
        {
            let mut s = state.write().unwrap();
            s.bitcoin.height = 100;
        }
        let res = get_state(State(state)).await;
        assert_eq!(res.0["bitcoin"]["height"], 100);
        assert_eq!(res.0["metrics"]["total_requests"], 1);
        assert!(res.0.as_object().unwrap().contains_key("uptime_seconds"));
    }

    #[tokio::test]
    async fn test_verify_attestation_handler() {
        use conxian_core::Attestation;
        let state = Arc::new(RwLock::new(GatewayState::default()));
        let attestation = Attestation {
            device_id: "conxius-123".to_string(),
            signature: "30440220263f69528d22384a32c2a07c3f3e1a8e9b6a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0220263f69528d22384a32c2a07c3f3e1a8e9b6a0a0a0a0a0a0a0a0a0a0a0a0a0a0a".to_string(),
            payload: "payload".to_string(),
            public_key: "0250863ad64a87ad8a2bf2bb8ae16617bc25e101c70628d01f0599a4f7bb4d602f".to_string(),
        };
        let res =
            verify_attestation(State(state), Json(AttestationRequest::Ecdsa(attestation))).await;
        assert!(res.is_err());
    }
}
