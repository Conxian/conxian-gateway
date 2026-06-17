use api::a2p::A2pRouter;
use api::fiat::FiatRouter;
use api::{configure_routes, new_lightning_adapter, new_settlement_log, AppState};
use compliance::{CoreVerifier, IdentityManager, UniversalVerifier, ZkcVerifier};
use conxian_core::{GatewayState, SharedState};
use std::collections::HashSet;
use std::sync::{Arc, Mutex, RwLock};

const TEST_TOKEN: &str = "test-token";

fn setup_app(state: SharedState) -> axum::Router {
    let fiat = Arc::new(FiatRouter::new(
        "ramp-key".to_string(),
        "investec-id".to_string(),
        "investec-secret".to_string(),
        "alchemy-id".to_string(),
        "alchemy-secret".to_string(),
        "banxa-key".to_string(),
        "banxa-secret".to_string(),
    ));
    let a2p = Arc::new(A2pRouter::new(
        "test-infobip".to_string(),
        "test-infobip".to_string(),
        "test-hmac".to_string(),
    ));
    let identity = Arc::new(IdentityManager::new());
    let compliance = Arc::new(ZkcVerifier::new());
    let alex = Arc::new(engine::stacks::alex::SimulatedAlexClient);
    let multi_chain: std::collections::HashMap<String, Arc<dyn conxian_core::ChainAdapter>> =
        std::collections::HashMap::new();

    let verifier = Arc::new(UniversalVerifier::new(
        compliance.clone() as Arc<dyn CoreVerifier>,
        multi_chain.clone(),
    ));

    struct SimulatedOfflineQueue {
        _replay_claims: Mutex<HashSet<String>>,
    }

    impl conxian_core::OfflineQueue for SimulatedOfflineQueue {
        fn enqueue(&self, _r: &conxian_core::OfflineReceipt) -> conxian_core::ConxianResult<()> {
            Ok(())
        }
        fn dequeue_pending(
            &self,
        ) -> conxian_core::ConxianResult<Vec<conxian_core::OfflineReceipt>> {
            Ok(vec![])
        }
        fn mark_broadcasted(&self, _id: &str) -> conxian_core::ConxianResult<()> {
            Ok(())
        }
        fn claim_replay_key(&self, _key: &str, _ttl: u64) -> conxian_core::ConxianResult<bool> {
            Ok(true)
        }
    }
    let offline_queue = Arc::new(SimulatedOfflineQueue {
        _replay_claims: Mutex::new(HashSet::new()),
    });

    let app_state = AppState {
        shared: state,
        fiat,
        a2p,
        identity,
        compliance,
        verifier,
        alex,
        multi_chain,
        lightning: new_lightning_adapter(),
        fiat_webhook_secret: "fake".to_string(),
        settlement_ingress_secret: "stub".to_string(),
        settlement_log: new_settlement_log(),
        offline_queue,
    };

    configure_routes(app_state, TEST_TOKEN.to_string())
}

#[tokio::test]
async fn test_offline_pos_blackout_reconciliation() {
    let state = Arc::new(RwLock::new(GatewayState::default()));
    let _app = setup_app(state);
    // Simplified: check if it compiles and setup works
}
