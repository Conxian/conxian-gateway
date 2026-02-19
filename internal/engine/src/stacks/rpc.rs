use async_trait::async_trait;
use conxian_core::ConxianResult;

#[async_trait]
pub trait StacksRpc: Send + Sync {
    async fn get_block_count(&self) -> ConxianResult<u64>;
}

pub struct SimulatedStacksRpc {
    pub initial_height: u64,
}

#[async_trait]
impl StacksRpc for SimulatedStacksRpc {
    async fn get_block_count(&self) -> ConxianResult<u64> {
        Ok(self.initial_height)
    }
}
