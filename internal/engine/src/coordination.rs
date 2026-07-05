use conxian_core::{ConxianError, ConxianResult};
use redis::{AsyncCommands, Client};
use tracing::info;

/// Authenticated Redis client for cross-gateway coordination and state consistency.
/// Addresses G-1276 requirement for authenticated state log.
pub struct RedisCoordinator {
    client: Client,
}

impl RedisCoordinator {
    pub fn new(url: &str) -> ConxianResult<Self> {
        let client = Client::open(url).map_err(|e| {
            ConxianError::Internal(format!("Failed to connect to Redis at {}: {}", url, e))
        })?;

        info!("Redis coordinator initialized at {}", url);
        Ok(Self { client })
    }

    pub async fn publish_state_root(&self, chain: &str, root: &str) -> ConxianResult<()> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| ConxianError::Internal(format!("Redis connection error: {}", e)))?;

        info!(chain = chain, root = root, "Publishing state root to Redis");

        let _: () = conn
            .set(format!("state_root:{}", chain), root)
            .await
            .map_err(|e| ConxianError::Internal(format!("Redis set error: {}", e)))?;

        let _: () = conn
            .publish("state_updates", format!("{}:{}", chain, root))
            .await
            .map_err(|e| ConxianError::Internal(format!("Redis publish error: {}", e)))?;

        Ok(())
    }

    pub async fn get_state_root(&self, chain: &str) -> ConxianResult<Option<String>> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| ConxianError::Internal(format!("Redis connection error: {}", e)))?;

        let val: Option<String> = conn
            .get(format!("state_root:{}", chain))
            .await
            .map_err(|e| ConxianError::Internal(format!("Redis get error: {}", e)))?;

        Ok(val)
    }
}
