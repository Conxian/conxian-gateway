use async_trait::async_trait;
use conxian_core::{
    BlockInfo, ConxianError, ConxianResult, GatewayState, Persistence, PersistentState, SharedState,
};
use engine::{BitcoinListener, BitcoinRpc};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Clone)]
struct ReorgSimulationRpc {
    tip: Arc<Mutex<SimulatedTip>>,
}

#[derive(Clone)]
struct SimulatedTip {
    height: u64,
    hash: String,
    timestamp: u64,
}

impl ReorgSimulationRpc {
    fn new(height: u64, hash: &str, timestamp: u64) -> Self {
        Self {
            tip: Arc::new(Mutex::new(SimulatedTip {
                height,
                hash: hash.to_string(),
                timestamp,
            })),
        }
    }

    fn set_tip(&self, height: u64, hash: &str, timestamp: u64) {
        let mut tip = self.tip.lock().unwrap();
        tip.height = height;
        tip.hash = hash.to_string();
        tip.timestamp = timestamp;
    }
}

#[async_trait]
impl BitcoinRpc for ReorgSimulationRpc {
    async fn get_block_count(&self) -> ConxianResult<u64> {
        Ok(self.tip.lock().unwrap().height)
    }

    async fn get_block_info(&self, height: u64) -> ConxianResult<BlockInfo> {
        let tip = self.tip.lock().unwrap().clone();
        if height != tip.height {
            return Err(ConxianError::Bitcoin(format!(
                "unexpected block info request for height {} (tip is {})",
                height, tip.height
            )));
        }

        Ok(BlockInfo {
            hash: tip.hash,
            height: tip.height,
            timestamp: tip.timestamp,
        })
    }

    async fn get_network_info(&self) -> ConxianResult<String> {
        Ok("regtest".to_string())
    }
}

struct InMemoryPersistence {
    state: Mutex<PersistentState>,
}

impl InMemoryPersistence {
    fn new() -> Self {
        Self {
            state: Mutex::new(PersistentState::default()),
        }
    }
}

impl Persistence for InMemoryPersistence {
    fn save(&self, state: &PersistentState) -> ConxianResult<()> {
        *self.state.lock().unwrap() = state.clone();
        Ok(())
    }

    fn load(&self) -> ConxianResult<PersistentState> {
        Ok(self.state.lock().unwrap().clone())
    }
}

#[tokio::test]
async fn bitcoin_listener_reorg_simulation_updates_tip_hash_at_same_height() {
    let state: SharedState = Arc::new(RwLock::new(GatewayState::default()));
    let rpc = ReorgSimulationRpc::new(700_000, "000000000000000000-tip-a", 1_710_000_001);
    let persistence = Arc::new(InMemoryPersistence::new());

    let mut listener =
        BitcoinListener::new(rpc.clone(), state.clone(), persistence.clone(), None, 30);
    listener.sync_once().await.unwrap();

    {
        let s = state.read().unwrap();
        assert_eq!(s.bitcoin.height, 700_000);
        assert_eq!(s.bitcoin.best_block_hash, "000000000000000000-tip-a");
        assert_eq!(s.bitcoin.status, "synced");
        assert_eq!(s.bitcoin.network, "regtest");
    }

    rpc.set_tip(700_000, "000000000000000000-tip-b", 1_710_000_222);
    listener.sync_once().await.unwrap();

    {
        let s = state.read().unwrap();
        assert_eq!(s.bitcoin.height, 700_000);
        assert_eq!(s.bitcoin.best_block_hash, "000000000000000000-tip-b");
        assert_eq!(s.bitcoin.last_updated, 1_710_000_222);
        assert_eq!(s.bitcoin.status, "synced");
    }

    let persisted = persistence.load().unwrap();
    assert_eq!(persisted.bitcoin_height, 700_000);
}
