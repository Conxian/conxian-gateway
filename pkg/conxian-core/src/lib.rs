use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockInfo {
    pub hash: String,
    pub height: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
    pub txid: String,
    pub confirmations: u32,
    pub block_hash: Option<String>,
    pub block_height: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainState {
    pub height: u64,
    pub status: String,
    pub last_updated: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GatewayState {
    pub bitcoin: ChainState,
    pub stacks: ChainState,
}

impl Default for ChainState {
    fn default() -> Self {
        Self {
            height: 0,
            status: "initializing".to_string(),
            last_updated: 0,
        }
    }
}

pub type SharedState = Arc<RwLock<GatewayState>>;

#[derive(Error, Debug)]
pub enum ConxianError {
    #[error("Bitcoin error: {0}")]
    Bitcoin(String),
    #[error("Stacks error: {0}")]
    Stacks(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("Compliance error: {0}")]
    Compliance(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

pub type ConxianResult<T> = Result<T, ConxianError>;
