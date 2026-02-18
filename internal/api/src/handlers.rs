use axum::{extract::State, Json};
use compliance::{Attestation, ZkcVerifier};
use conxian_core::SharedState;
use serde_json::{json, Value};

pub async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "service": "conxian-gateway",
        "version": "0.1.0"
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
