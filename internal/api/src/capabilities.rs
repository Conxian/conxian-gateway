//! Discovery of registrations in this process, without inferring live support.
use crate::AppState;
use axum::{extract::State, http::header, Json};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AdapterRegistration {
    pub adapter_id: String,
    pub runtime_state: &'static str,
    pub health: &'static str,
    pub production_support: &'static str,
    pub qualified_operations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AdapterDiscoveryManifest {
    pub schema_version: u32,
    pub manifest_kind: &'static str,
    pub repository: &'static str,
    pub version: &'static str,
    pub trust_root: &'static str,
    pub scope: &'static str,
    pub native_conxian_token: bool,
    pub source: &'static str,
    pub adapters: Vec<AdapterRegistration>,
}

pub fn discover_registered_adapters(
    ids: impl IntoIterator<Item = String>,
) -> AdapterDiscoveryManifest {
    AdapterDiscoveryManifest {
        schema_version: 1,
        manifest_kind: "adapter_registration",
        repository: "Conxian/conxian-gateway",
        version: env!("CARGO_PKG_VERSION"),
        trust_root: "bitcoin-l1",
        scope: "universal-chain",
        native_conxian_token: false,
        source: "runtime-adapter-map",
        adapters: ids
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|adapter_id| AdapterRegistration {
                adapter_id,
                runtime_state: "registered",
                health: "not-observed",
                production_support: "not-evidenced",
                qualified_operations: Vec::new(),
            })
            .collect(),
    }
}

/// The capability ids advertised by the gateway: the runtime chain adapters plus
/// the always-mounted `dlc` settlement-rail capability.
pub fn advertised_capability_ids(
    chain_ids: impl IntoIterator<Item = String>,
) -> impl Iterator<Item = String> {
    chain_ids
        .into_iter()
        .chain(std::iter::once("dlc".to_string()))
}

pub async fn get_capabilities(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    // The manifest advertises the registered capability/adapter set. Chain
    // adapters come from the runtime map; `dlc` is a settlement-rail capability
    // (the adaptor-signature CET path served at `POST /api/v1/dlc/cet`), which
    // is always mounted. It is advertised as a capability id so a consumer can
    // gate on it without inferring production support.
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(discover_registered_adapters(advertised_capability_ids(
            state.multi_chain.keys().cloned(),
        ))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_is_sorted_deduplicated_and_never_infers_qualification() {
        let manifest = discover_registered_adapters(["evm", "bitcoin", "evm"].map(String::from));
        assert_eq!(
            manifest
                .adapters
                .iter()
                .map(|a| a.adapter_id.as_str())
                .collect::<Vec<_>>(),
            ["bitcoin", "evm"]
        );
        assert!(manifest.adapters.iter().all(|a| a.health == "not-observed"
            && a.production_support == "not-evidenced"
            && a.qualified_operations.is_empty()));
    }

    #[test]
    fn an_empty_runtime_does_not_gain_registry_support() {
        assert!(discover_registered_adapters(Vec::new()).adapters.is_empty());
    }

    #[test]
    fn dlc_capability_is_always_advertised_and_sorted() {
        let manifest = discover_registered_adapters(advertised_capability_ids(
            ["evm", "bitcoin"].map(String::from),
        ));
        assert_eq!(
            manifest
                .adapters
                .iter()
                .map(|a| a.adapter_id.as_str())
                .collect::<Vec<_>>(),
            ["bitcoin", "dlc", "evm"]
        );
        // Even with no runtime chains, the dlc capability is still advertised.
        let manifest = discover_registered_adapters(advertised_capability_ids(Vec::new()));
        assert_eq!(
            manifest
                .adapters
                .iter()
                .map(|a| a.adapter_id.as_str())
                .collect::<Vec<_>>(),
            ["dlc"]
        );
    }
}
