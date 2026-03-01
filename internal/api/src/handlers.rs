use axum::{extract::State, http::StatusCode, Json};
use compliance::ZkcVerifier;
use conxian_core::{AttestationRequest, SharedState};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn health_check(State(state): State<SharedState>) -> Json<Value> {
    let s = state.read().unwrap();
    let mut status = "healthy";
    let mut details = Vec::new();

    if s.bitcoin.status.contains("error") {
        status = "degraded";
        details.push(format!("Bitcoin: {}", s.bitcoin.status));
    }
    if s.stacks.status.contains("error") {
        status = "degraded";
        details.push(format!("Stacks: {}", s.stacks.status));
    }

    {
        let mut s_write = state.write().unwrap();
        s_write.metrics.total_requests += 1;
        s_write.metrics.health_requests += 1;
    }

    Json(json!({
        "status": status,
        "service": "conxian-gateway",
        "version": conxian_core::VERSION,
        "details": if details.is_empty() { None } else { Some(details) }
    }))
}

pub async fn get_state(State(state): State<SharedState>) -> Json<Value> {
    {
        let mut s = state.write().unwrap();
        s.metrics.total_requests += 1;
        s.metrics.state_requests += 1;
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
    let mut s_write = state.write().unwrap();
    s_write.metrics.total_requests += 1;
    s_write.metrics.metrics_requests += 1;
    drop(s_write);

    let s = state.read().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let uptime = now.saturating_sub(s.start_time);

    format!(
        "# HELP gateway_total_requests The total number of API requests received.\n\
         # TYPE gateway_total_requests counter\n\
         gateway_total_requests {}\n\
         # HELP gateway_health_requests The number of health check requests.\n\
         # TYPE gateway_health_requests counter\n\
         gateway_health_requests {}\n\
         # HELP gateway_state_requests The number of state requests.\n\
         # TYPE gateway_state_requests counter\n\
         gateway_state_requests {}\n\
         # HELP gateway_metrics_requests The number of metrics requests.\n\
         # TYPE gateway_metrics_requests counter\n\
         gateway_metrics_requests {}\n\
         # HELP gateway_verification_requests The total number of attestation verifications attempted.\n\
         # TYPE gateway_verification_requests counter\n\
         gateway_verification_requests {}\n\
         # HELP gateway_verification_success The number of successful attestation verifications.\n\
         # TYPE gateway_verification_success counter\n\
         gateway_verification_success {}\n\
         # HELP gateway_verification_failure The number of failed attestation verifications.\n\
         # TYPE gateway_verification_failure counter\n\
         gateway_verification_failure {}\n\
         # HELP bitcoin_block_height The current block height of the Bitcoin chain.\n\
         # TYPE bitcoin_block_height gauge\n\
         bitcoin_block_height {}\n\
         # HELP stacks_block_height The current block height of the Stacks chain.\n\
         # TYPE stacks_block_height gauge\n\
         stacks_block_height {}\n\
         # HELP gateway_uptime_seconds The total uptime of the gateway in seconds.\n\
         # TYPE gateway_uptime_seconds counter\n\
         gateway_uptime_seconds {}\n",
        s.metrics.total_requests,
        s.metrics.health_requests,
        s.metrics.state_requests,
        s.metrics.metrics_requests,
        s.metrics.verification_requests,
        s.metrics.verification_success,
        s.metrics.verification_failure,
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
        s.metrics.verification_requests += 1;
    }

    let verifier = ZkcVerifier::new();
    let result = match request {
        AttestationRequest::Ecdsa(a) => verifier.verify(&a),
        AttestationRequest::Schnorr(a) => verifier.verify_schnorr(&a),
    };

    match result {
        Ok(valid) => {
            {
                let mut s = state.write().unwrap();
                if valid {
                    s.metrics.verification_success += 1;
                } else {
                    s.metrics.verification_failure += 1;
                }
            }
            Ok(Json(json!({ "valid": valid })))
        }
        Err(e) => {
            {
                let mut s = state.write().unwrap();
                s.metrics.verification_failure += 1;
            }
            Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": e.to_string() })),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conxian_core::GatewayState;
    use std::sync::{Arc, RwLock};

    #[tokio::test]
    async fn test_health_check_handler() {
        let state = Arc::new(RwLock::new(GatewayState::default()));
        let res = health_check(State(state)).await;
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
        assert_eq!(res.0["metrics"]["state_requests"], 1);
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
