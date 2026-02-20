use axum::{extract::State, Json};
use compliance::ZkcVerifier;
use conxian_core::{Attestation, SharedState};
use serde_json::{json, Value};

pub async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "service": "conxian-gateway",
        "version": conxian_core::VERSION
    }))
}

pub async fn get_state(State(state): State<SharedState>) -> Json<Value> {
    let state = state.read().unwrap();
    Json(json!({
        "bitcoin": state.bitcoin,
        "stacks": state.stacks
    }))
}

pub async fn verify_attestation(
    Json(attestation): Json<Attestation>,
) -> Result<Json<Value>, Json<Value>> {
    let verifier = ZkcVerifier::new();
    match verifier.verify(&attestation) {
        Ok(valid) => Ok(Json(json!({ "valid": valid }))),
        Err(e) => Err(Json(json!({ "error": e.to_string() }))),
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
    }

    #[tokio::test]
    async fn test_verify_attestation_handler() {
        let attestation = Attestation {
            device_id: "conxius-123".to_string(),
            signature: "30440220263f69528d22384a32c2a07c3f3e1a8e9b6a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0220263f69528d22384a32c2a07c3f3e1a8e9b6a0a0a0a0a0a0a0a0a0a0a0a0a0a0a".to_string(),
            payload: "payload".to_string(),
            public_key: "0250863ad64a87ad8a2bf2bb8ae16617bc25e101c70628d01f0599a4f7bb4d602f".to_string(),
        };
        let res = verify_attestation(Json(attestation)).await;
        assert!(res.is_err());
    }
}
