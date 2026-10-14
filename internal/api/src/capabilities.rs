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

pub fn discover_registered_adapters(ids: impl IntoIterator<Item = String>) -> AdapterDiscoveryManifest {
    AdapterDiscoveryManifest {
        schema_version: 1,
        manifest_kind: "adapter_registration",
        repository: "Conxian/conxian-gateway",
        version: env!("CARGO_PKG_VERSION"),
        trust_root: "bitcoin-l1",
        scope: "universal-chain",
        native_conxian_token: false,
        source: "runtime-adapter-map",
        adapters: ids.into_iter().collect::<BTreeSet<_>>().into_iter().map(|adapter_id| AdapterRegistration {
            adapter_id,
            runtime_state: "registered",
            health: "not-observed",
            production_support: "not-evidenced",
            qualified_operations: Vec::new(),
        }).collect(),
    }
}

pub async fn get_capabilities(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    ([(header::CACHE_CONTROL, "no-store")], Json(discover_registered_adapters(state.multi_chain.keys().cloned())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_is_sorted_deduplicated_and_never_infers_qualification() {
        let manifest = discover_registered_adapters(["evm", "bitcoin", "evm"].map(String::from));
        assert_eq!(manifest.adapters.iter().map(|a| a.adapter_id.as_str()).collect::<Vec<_>>(), ["bitcoin", "evm"]);
        assert!(manifest.adapters.iter().all(|a| a.health == "not-observed" && a.production_support == "not-evidenced" && a.qualified_operations.is_empty()));
    }

    #[test]
    fn an_empty_runtime_does_not_gain_registry_support() {
        assert!(discover_registered_adapters(Vec::new()).adapters.is_empty());
    }
}
