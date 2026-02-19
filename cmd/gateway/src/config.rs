use std::env;

pub struct Config {
    pub bitcoin_rpc_url: String,
    pub bitcoin_rpc_user: String,
    pub bitcoin_rpc_pass: String,
    pub api_port: u16,
    pub api_token: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            bitcoin_rpc_url: env::var("BITCOIN_RPC_URL")
                .unwrap_or_else(|_| "http://localhost:18332".to_string()),
            bitcoin_rpc_user: env::var("BITCOIN_RPC_USER").unwrap_or_else(|_| "user".to_string()),
            bitcoin_rpc_pass: env::var("BITCOIN_RPC_PASS").unwrap_or_else(|_| "pass".to_string()),
            api_port: env::var("API_PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .unwrap_or(3000),
            api_token: env::var("API_TOKEN").unwrap_or_else(|_| "institutional-default-token".to_string()),
        }
    }
}
