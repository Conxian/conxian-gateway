use conxian_core::{
    evaluate_trust_metadata_json, ConxianResult, SharedState, TrustPolicyDecision,
    TrustPolicyReasonCode,
};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

const NTT_TRUST_METADATA_ENV: &str = "CONXIAN_NTT_TRUST_METADATA";

pub struct NttRelayer {
    state: SharedState,
    poll_interval: u64,
}

impl NttRelayer {
    pub fn new(state: SharedState, poll_interval: u64) -> Self {
        Self {
            state,
            poll_interval,
        }
    }

    pub async fn run(&self) -> ConxianResult<()> {
        info!("Starting NTT Relayer for sovereign bridging...");

        loop {
            if let Err(e) = self.process_ntt_events().await {
                warn!("Error processing NTT events: {}", e);
            }
            sleep(Duration::from_secs(self.poll_interval)).await;
        }
    }

    async fn process_ntt_events(&self) -> ConxianResult<()> {
        let height = {
            let s = self.state.read().unwrap();
            s.stacks.height
        };

        if height > 0 && height % 10 == 0 {
            info!(
                "NTT event detected at Stacks height {}. Submitting VAA to destination...",
                height
            );
            self.submit_vaa(height).await?;
        }

        Ok(())
    }

    async fn submit_vaa(&self, source_height: u64) -> ConxianResult<()> {
        let decision = self.evaluate_trust_policy_from_env();
        self.record_trust_policy_metric(decision);

        if let TrustPolicyDecision::Block(reason) = decision {
            warn!(
                code = reason.as_str(),
                source_height,
                env_var = NTT_TRUST_METADATA_ENV,
                "NTT submit skipped by fail-closed trust policy"
            );
            return Ok(());
        }

        info!(
            "Sovereign VAA submitted for height {}. Bridge status: Active",
            source_height
        );
        Ok(())
    }

    fn evaluate_trust_policy_from_env(&self) -> TrustPolicyDecision {
        let now_epoch_secs = unix_epoch_secs();
        match std::env::var(NTT_TRUST_METADATA_ENV) {
            Ok(raw_metadata) => evaluate_trust_metadata_json(Some(&raw_metadata), now_epoch_secs),
            Err(std::env::VarError::NotPresent) => {
                evaluate_trust_metadata_json(None, now_epoch_secs)
            }
            Err(std::env::VarError::NotUnicode(_)) => {
                TrustPolicyDecision::Block(TrustPolicyReasonCode::MetadataInvalid)
            }
        }
    }

    fn record_trust_policy_metric(&self, decision: TrustPolicyDecision) {
        if let Ok(mut state) = self.state.write() {
            match decision {
                TrustPolicyDecision::Allow => state.metrics.trust_policy_allow += 1,
                TrustPolicyDecision::Block(_) => state.metrics.trust_policy_block += 1,
            }
        }
    }
}

fn unix_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use conxian_core::GatewayState;
    use serde_json::json;
    use std::sync::{Arc, Mutex, RwLock};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn set_env_var(key: &str, value: String) {
        #[allow(unused_unsafe)]
        unsafe {
            std::env::set_var(key, value);
        }
    }

    fn remove_env_var(key: &str) {
        #[allow(unused_unsafe)]
        unsafe {
            std::env::remove_var(key);
        }
    }

    fn trust_metadata(system: &str, trust_tier: &str) -> String {
        let now = unix_epoch_secs();
        serde_json::to_string(&json!({
            "system": system,
            "trust_tier": trust_tier,
            "policy": {
                "policy_id": "CON-791",
                "policy_version": "2026-06-01"
            },
            "evidence": {
                "source": "ntt-relayer-test",
                "reference": "test"
            },
            "freshness": {
                "observed_at_epoch_secs": now,
                "max_age_secs": 300
            }
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn submit_vaa_blocks_when_metadata_is_denied() {
        let _guard = ENV_LOCK.lock().unwrap();
        let metadata = trust_metadata("WORMHOLE_NTT", "T4");
        set_env_var(NTT_TRUST_METADATA_ENV, metadata);

        let state = Arc::new(RwLock::new(GatewayState::default()));
        let relayer = NttRelayer::new(state.clone(), 1);
        relayer.submit_vaa(10).await.unwrap();

        let s = state.read().unwrap();
        assert_eq!(s.metrics.trust_policy_allow, 0);
        assert_eq!(s.metrics.trust_policy_block, 1);

        remove_env_var(NTT_TRUST_METADATA_ENV);
    }

    #[tokio::test]
    async fn submit_vaa_allows_when_metadata_passes_policy() {
        let _guard = ENV_LOCK.lock().unwrap();
        let metadata = trust_metadata("WORMHOLE_NTT", "T2");
        set_env_var(NTT_TRUST_METADATA_ENV, metadata);

        let state = Arc::new(RwLock::new(GatewayState::default()));
        let relayer = NttRelayer::new(state.clone(), 1);
        relayer.submit_vaa(10).await.unwrap();

        let s = state.read().unwrap();
        assert_eq!(s.metrics.trust_policy_allow, 1);
        assert_eq!(s.metrics.trust_policy_block, 0);

        remove_env_var(NTT_TRUST_METADATA_ENV);
    }
}
