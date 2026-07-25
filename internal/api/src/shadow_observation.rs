use crate::AppState;
use axum::{extract::State, http::StatusCode, Json};
use conxian_engine::{BitcoinCoreShadowObservation, ObservationErrorCategory};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const SHADOW_OBSERVATION_SCHEMA_VERSION: u8 = 1;
pub const SHADOW_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShadowObservationMode {
    Shadow,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShadowObservationScope {
    BitcoinCoreConfiguredEndpoint,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShadowObservationDecisionUse {
    ObservationOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShadowObservationProvenance {
    pub source: ShadowObservationSource,
    pub scope: ShadowObservationScope,
    pub network_scope: ShadowObservationNetworkScope,
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShadowObservationSource {
    ConfiguredBitcoinCoreEndpoint,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShadowObservationNetworkScope {
    NotBitcoinNetwork,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShadowObservationResponse {
    pub schema_version: u8,
    pub mode: ShadowObservationMode,
    pub scope: ShadowObservationScope,
    pub decision_use: ShadowObservationDecisionUse,
    pub observed_at_unix: u64,
    pub provenance: ShadowObservationProvenance,
    #[serde(flatten)]
    pub observation: BitcoinCoreShadowObservation,
}

pub async fn get_bitcoin_core_shadow_observation(
    State(state): State<AppState>,
) -> Result<Json<ShadowObservationResponse>, (StatusCode, Json<Value>)> {
    let observer = state
        .bitcoin_core_shadow_observer
        .ok_or_else(disabled_error)?;
    let observation = tokio::time::timeout(SHADOW_OBSERVATION_TIMEOUT, observer.observe())
        .await
        .map_err(|_| unavailable_error(ObservationErrorCategory::Internal))?
        .map_err(|failure| unavailable_error(failure.category))?;

    Ok(Json(ShadowObservationResponse {
        schema_version: SHADOW_OBSERVATION_SCHEMA_VERSION,
        mode: ShadowObservationMode::Shadow,
        scope: ShadowObservationScope::BitcoinCoreConfiguredEndpoint,
        decision_use: ShadowObservationDecisionUse::ObservationOnly,
        observed_at_unix: now_unix(),
        provenance: ShadowObservationProvenance {
            source: ShadowObservationSource::ConfiguredBitcoinCoreEndpoint,
            scope: ShadowObservationScope::BitcoinCoreConfiguredEndpoint,
            network_scope: ShadowObservationNetworkScope::NotBitcoinNetwork,
            read_only: true,
        },
        observation,
    }))
}

fn disabled_error() -> (StatusCode, Json<Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "bitcoin_core_shadow_observation_disabled" })),
    )
}

fn unavailable_error(_failure: ObservationErrorCategory) -> (StatusCode, Json<Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "bitcoin_core_shadow_observation_unavailable" })),
    )
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_errors_do_not_expose_raw_failures() {
        let disabled = disabled_error().1 .0;
        let unavailable = unavailable_error(ObservationErrorCategory::Transport).1 .0;
        assert_eq!(
            disabled,
            json!({ "error": "bitcoin_core_shadow_observation_disabled" })
        );
        assert_eq!(
            unavailable,
            json!({ "error": "bitcoin_core_shadow_observation_unavailable" })
        );
        for body in [disabled, unavailable] {
            let serialized = body.to_string();
            assert!(!serialized.contains("raw"));
            assert!(!serialized.contains("credential"));
            assert!(!serialized.contains("rpc_url"));
        }
    }
}
