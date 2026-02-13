use axum::Json;
use serde_json::{json, Value};
use compliance::{Attestation, ZkcVerifier};

pub async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "service": "conxian-gateway",
        "version": "0.1.0"
    }))
}

pub async fn get_state() -> Json<Value> {
    Json(json!({
        "bitcoin": {
            "status": "synced",
            "height": 800000
        },
        "stacks": {
            "status": "synced",
            "height": 100000
        }
    }))
}

pub async fn verify_attestation(Json(attestation): Json<Attestation>) -> Result<Json<Value>, Json<Value>> {
    let verifier = ZkcVerifier::new();
    match verifier.verify(&attestation) {
        Ok(valid) => Ok(Json(json!({ "valid": valid }))),
        Err(e) => Err(Json(json!({ "error": e.to_string() }))),
    }
}
