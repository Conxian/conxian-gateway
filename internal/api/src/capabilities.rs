//! Discovery of registrations in this process, without inferring live support.
use crate::AppState;
use axum::{extract::State, http::header, Json};
use lib_conxian_core::capability::CHAIN_TAXONOMY_VERSION;
use lib_conxian_core::control_model::{chain_family_for, Chain, ALL_CHAINS};
use serde::Serialize;
use std::collections::BTreeSet;

/// Capability ids that are always mounted regardless of the runtime chain map.
/// `dlc` is the settlement-rail capability served at `POST /api/v1/dlc/cet`.
const ALWAYS_ON_CAPABILITIES: &[&str] = &["dlc"];

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AdapterRegistration {
    pub adapter_id: String,
    pub runtime_state: &'static str,
    pub health: &'static str,
    pub production_support: &'static str,
    pub qualified_operations: Vec<String>,
    /// Operation classes this adapter can perform, classified against the
    /// gateway `ChainAdapter` surface. Adapters with no Core taxonomy binding
    /// are reported `unavailable` rather than inferred as routable.
    pub operation_classes: Vec<String>,
    /// Bound Core taxonomy identity, when the adapter id maps to a known chain.
    pub chain: Option<String>,
    pub family: Option<String>,
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
    /// Version of the Core chain taxonomy this manifest is bound to.
    pub taxonomy_version: &'static str,
    pub adapters: Vec<AdapterRegistration>,
}

/// Render an enum's `snake_case` serde name (the wire identity), if any.
fn snake(value: &impl serde::Serialize) -> Option<String> {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
}

/// Resolve a runtime adapter id to a Core taxonomy chain, when one exists.
fn chain_for_adapter_id(adapter_id: &str) -> Option<Chain> {
    ALL_CHAINS
        .iter()
        .find(|chain| snake(chain).as_deref() == Some(adapter_id))
        .cloned()
}

/// Classify an adapter's operation classes without inferring live support.
fn classify(adapter_id: &str) -> (Vec<String>, Option<String>, Option<String>) {
    if ALWAYS_ON_CAPABILITIES.contains(&adapter_id) {
        // `dlc` is a settlement-rail capability: it prepares/signs a CET via the
        // adaptor-signature path but does not observe or route a chain.
        return (vec!["delegated_execute".to_string()], None, None);
    }
    match chain_for_adapter_id(adapter_id) {
        Some(chain) => {
            let family = chain_family_for(&chain);
            (
                vec![
                    "observe".to_string(),
                    "route".to_string(),
                    "prepare".to_string(),
                ],
                snake(&chain),
                snake(&family),
            )
        }
        None => (vec!["unavailable".to_string()], None, None),
    }
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
        taxonomy_version: CHAIN_TAXONOMY_VERSION,
        adapters: ids
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|adapter_id| {
                let (operation_classes, chain, family) = classify(&adapter_id);
                AdapterRegistration {
                    adapter_id,
                    runtime_state: "registered",
                    health: "not-observed",
                    production_support: "not-evidenced",
                    qualified_operations: Vec::new(),
                    operation_classes,
                    chain,
                    family,
                }
            })
            .collect(),
    }
}

/// The capability ids advertised by the gateway: the runtime chain adapters plus
/// the always-mounted settlement-rail capabilities.
pub fn advertised_capability_ids(
    chain_ids: impl IntoIterator<Item = String>,
) -> impl Iterator<Item = String> {
    chain_ids.into_iter().chain(
        ALWAYS_ON_CAPABILITIES
            .iter()
            .map(|capability| capability.to_string()),
    )
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

    #[test]
    fn manifest_is_bound_to_the_core_taxonomy_version() {
        let manifest = discover_registered_adapters(Vec::new());
        assert_eq!(manifest.taxonomy_version, CHAIN_TAXONOMY_VERSION);
    }

    #[test]
    fn known_chain_adapter_is_bound_to_taxonomy_with_operation_classes() {
        let manifest = discover_registered_adapters(["citrea", "babylon"].map(String::from));
        let citrea = manifest
            .adapters
            .iter()
            .find(|a| a.adapter_id == "citrea")
            .expect("citrea present");
        assert_eq!(citrea.chain.as_deref(), Some("citrea"));
        assert!(citrea.family.is_some());
        assert_eq!(citrea.operation_classes, ["observe", "route", "prepare"]);
    }

    #[test]
    fn unmapped_adapter_is_unavailable_not_routable() {
        // `bitvm3` has no Core taxonomy binding; it must not be inferred routable.
        let manifest = discover_registered_adapters(["bitvm3"].map(String::from));
        let adapter = &manifest.adapters[0];
        assert_eq!(adapter.operation_classes, ["unavailable"]);
        assert!(adapter.chain.is_none());
        assert!(adapter.family.is_none());
    }

    #[test]
    fn dlc_is_delegated_execute_only() {
        let manifest = discover_registered_adapters(["dlc"].map(String::from));
        assert_eq!(
            manifest.adapters[0].operation_classes,
            ["delegated_execute"]
        );
        assert!(manifest.adapters[0].chain.is_none());
    }
}
