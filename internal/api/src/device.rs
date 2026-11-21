//! Fleet-agent server side (Track B of the device lifecycle).
//!
//! The server holds *desired state* and *signs manifests*; it never runs
//! anything on a device. Two endpoints make the pull loop work:
//!
//! * `GET  /api/v1/devices/{id}/desired-state` — returns the signed manifest
//!   for the device, or an empty (converged) manifest when none is set.
//! * `POST /api/v1/devices/{id}/report` — records the device's apply outcome.
//!
//! Both require the device's enrollment credential. The manifest is signed with
//! the operator's signing key (`DEVICE_MANIFEST_SIGNING_KEY`), and the device
//! verifies it against its pinned copy of that key — so a compromised transport
//! cannot forge a manifest.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use conxian_device::{sign_manifest, ActionKind, DesiredStateManifest, SignedManifest};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;

/// Durable-ish store of per-device desired state and last report.
///
/// Kept in-process (the desired-state set is small and operator-managed); the
/// reports are the audit trail. A Postgres-backed store can replace this
/// without changing the handlers.
#[derive(Default)]
pub struct DeviceRegistry {
    desired: RwLock<std::collections::HashMap<String, DesiredStateManifest>>,
    reports: RwLock<Vec<DeviceReport>>,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn set_desired(&self, device: &str, manifest: DesiredStateManifest) {
        self.desired
            .write()
            .await
            .insert(device.to_string(), manifest);
    }

    pub async fn desired(&self, device: &str) -> Option<DesiredStateManifest> {
        self.desired.read().await.get(device).cloned()
    }

    pub async fn record(&self, report: DeviceReport) {
        self.reports.write().await.push(report);
    }

    pub async fn report_count(&self) -> usize {
        self.reports.read().await.len()
    }
}

/// A device's reported apply outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceReport {
    pub device: String,
    pub manifest_digest: String,
    pub status: String,
    #[serde(default)]
    pub outcomes: Vec<String>,
}

/// Shared state for the device routes.
#[derive(Clone)]
pub struct DeviceApi {
    pub registry: Arc<DeviceRegistry>,
    /// Signing key (hex) for desired-state manifests.
    signing_key: String,
    /// The enrollment credential every device must present.
    enrollment_token: String,
}

impl DeviceApi {
    /// Builds the device API from the environment, or `None` when either the
    /// signing key or the enrollment token is unset. The routes then do not
    /// mount, mirroring the onboarding/webhook fail-closed behaviour.
    pub fn from_env(registry: Arc<DeviceRegistry>) -> Option<Self> {
        let signing_key = std::env::var("DEVICE_MANIFEST_SIGNING_KEY")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())?;
        let enrollment_token = std::env::var("DEVICE_ENROLLMENT_TOKEN")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())?;
        Some(Self {
            registry,
            signing_key,
            enrollment_token,
        })
    }

    fn token_ok(&self, headers: &HeaderMap) -> bool {
        let presented = headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "));
        match presented {
            Some(token) => {
                token
                    .as_bytes()
                    .ct_eq(self.enrollment_token.as_bytes())
                    .unwrap_u8()
                    == 1
            }
            None => false,
        }
    }

    /// Sign a manifest for a device. Exposed so the operator (or a test) can
    /// produce the same bytes the device will verify.
    pub fn sign(&self, manifest: &DesiredStateManifest) -> Result<SignedManifest, String> {
        sign_manifest(manifest, &self.signing_key).map_err(|e| e.to_string())
    }
}

/// `GET /api/v1/devices/{id}/desired-state` — the signed manifest for a device.
pub async fn get_desired_state(
    State(api): State<DeviceApi>,
    Path(device_id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !api.token_ok(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid enrollment credential" })),
        )
            .into_response();
    }

    // A device with no desired state converges to an empty manifest: the pull
    // loop then reports "noop" and the device keeps running.
    let manifest =
        api.registry.desired(&device_id).await.unwrap_or_else(|| {
            DesiredStateManifest::new(&device_id, chrono::Utc::now().to_rfc3339())
        });

    match api.sign(&manifest) {
        Ok(signed) => (StatusCode::OK, Json(signed)).into_response(),
        Err(e) => {
            tracing::error!(device = %device_id, error = %e, "failed to sign desired-state manifest");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "manifest signing failed" })),
            )
                .into_response()
        }
    }
}

/// `POST /api/v1/devices/{id}/report` — record a device's apply outcome.
pub async fn post_report(
    State(api): State<DeviceApi>,
    Path(device_id): Path<String>,
    headers: HeaderMap,
    Json(mut report): Json<DeviceReport>,
) -> impl IntoResponse {
    if !api.token_ok(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid enrollment credential" })),
        )
            .into_response();
    }
    // The path is authoritative for identity; never trust a body that disagrees.
    report.device = device_id;
    api.registry.record(report).await;
    (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
}

/// `POST /api/v1/devices/{id}/desired-state` — operator sets desired state.
///
/// Gated by the enrollment token for now; a separate operator token can be
/// layered on without changing the device-facing GET.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetDesiredRequest {
    #[serde(default)]
    pub actions: Vec<ActionKind>,
}

pub async fn put_desired_state(
    State(api): State<DeviceApi>,
    Path(device_id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<SetDesiredRequest>,
) -> impl IntoResponse {
    if !api.token_ok(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid enrollment credential" })),
        )
            .into_response();
    }
    let mut manifest = DesiredStateManifest::new(&device_id, chrono::Utc::now().to_rfc3339());
    manifest.actions = req.actions;
    api.registry.set_desired(&device_id, manifest).await;
    (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_is_none_without_keys() {
        std::env::remove_var("DEVICE_MANIFEST_SIGNING_KEY");
        std::env::remove_var("DEVICE_ENROLLMENT_TOKEN");
        assert!(DeviceApi::from_env(Arc::new(DeviceRegistry::new())).is_none());
    }

    #[test]
    fn sign_produces_a_verifiable_manifest() {
        std::env::set_var(
            "DEVICE_MANIFEST_SIGNING_KEY",
            "1111111111111111111111111111111111111111111111111111111111111111",
        );
        std::env::set_var("DEVICE_ENROLLMENT_TOKEN", "tok");
        let api = DeviceApi::from_env(Arc::new(DeviceRegistry::new())).unwrap();
        let mut m = DesiredStateManifest::new("d1", "2026-11-14T00:00:00Z");
        m.actions.push(ActionKind::RestartService {
            service: "gateway".into(),
        });
        let signed = api.sign(&m).unwrap();
        conxian_device::verify_manifest(&signed, &signed.signer, None).unwrap();
        std::env::remove_var("DEVICE_MANIFEST_SIGNING_KEY");
        std::env::remove_var("DEVICE_ENROLLMENT_TOKEN");
    }
}
