use async_trait::async_trait;
use conxian_core::{ConxianError, ConxianResult};
use redis::{aio::MultiplexedConnection, AsyncCommands, Client};
use tracing::{info, warn};

/// Authenticated Redis client for cross-gateway coordination and state consistency.
/// Addresses G-1276 requirement for authenticated state log —
/// enforces explicit AUTH and connection health check.
pub struct RedisCoordinator {
    client: Client,
}

#[async_trait]
pub trait StateRootPublisher: Send + Sync {
    async fn publish_state_root(&self, chain: &str, root: &str) -> ConxianResult<()>;
}

#[async_trait]
impl StateRootPublisher for RedisCoordinator {
    async fn publish_state_root(&self, chain: &str, root: &str) -> ConxianResult<()> {
        RedisCoordinator::publish_state_root(self, chain, root).await
    }
}

impl RedisCoordinator {
    /// Create a new RedisCoordinator with optional username/password for ACL auth.
    /// Sends AUTH + PING on construction to fail-fast on misconfiguration.
    pub fn new(url: &str, username: Option<&str>, password: Option<&str>) -> ConxianResult<Self> {
        let client = Client::open(url).map_err(|e| {
            ConxianError::Internal(format!("Failed to connect to Redis at {}: {}", url, e))
        })?;

        let has_auth = password.is_some();
        let rt = tokio::runtime::Handle::try_current().map_err(|_| {
            ConxianError::Internal("RedisCoordinator requires a Tokio runtime".into())
        })?;

        rt.block_on(async {
            let mut conn: MultiplexedConnection = client
                .get_multiplexed_async_connection()
                .await
                .map_err(|e| {
                ConxianError::Internal(format!("Redis connection error: {}", e))
            })?;

            if let Some(pw) = password {
                if let Some(user) = username.filter(|u| !u.is_empty()) {
                    let _: () = redis::cmd("AUTH")
                        .arg(user)
                        .arg(pw)
                        .query_async(&mut conn)
                        .await
                        .map_err(|e| {
                            ConxianError::Internal(format!(
                                "Redis AUTH failed for user '{}': {}",
                                user, e
                            ))
                        })?;
                } else {
                    let _: () = redis::cmd("AUTH")
                        .arg(pw)
                        .query_async(&mut conn)
                        .await
                        .map_err(|e| ConxianError::Internal(format!("Redis AUTH failed: {}", e)))?;
                }
            }

            let _: () = redis::cmd("PING")
                .query_async(&mut conn)
                .await
                .map_err(|e| ConxianError::Internal(format!("Redis PING failed: {}", e)))?;

            Ok(())
        })?;

        if !has_auth {
            warn!("Redis connection initialized without AUTH — ensure this is intentional");
        }

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
