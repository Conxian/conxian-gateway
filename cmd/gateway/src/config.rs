use std::env;

#[allow(dead_code)]
pub struct Config {
    pub bitcoin_rpc_url: String,
    pub bitcoin_rpc_user: String,
    pub bitcoin_rpc_pass: String,
    pub bitcoin_sync_interval: u64,
    pub stacks_rpc_url: String,
    pub stacks_sync_interval: u64,
    pub api_port: u16,
    pub api_token: String,
    pub ramp_api_key: String,
    pub investec_client_id: String,
    pub investec_secret: String,
    pub alchemy_pay_app_id: String,
    pub alchemy_pay_secret: String,
    pub banxa_api_key: String,
    pub banxa_secret: String,
    pub infobip_api_key: String,
    pub infobip_base_url: String,
    pub hmac_secret: String,
    pub fiat_webhook_secret: String,
    pub settlement_ingress_secret: String,
}

impl Config {
    pub fn from_env() -> Self {
        let fiat_webhook_secret =
            env::var("FIAT_WEBHOOK_SECRET").unwrap_or_else(|_| "default-fiat-secret".to_string());
        let settlement_ingress_secret =
            env::var("SETTLEMENT_INGRESS_SECRET").unwrap_or_else(|_| fiat_webhook_secret.clone());

        Self {
            bitcoin_rpc_url: env::var("BITCOIN_RPC_URL")
                .unwrap_or_else(|_| "http://localhost:18332".to_string()),
            bitcoin_rpc_user: env::var("BITCOIN_RPC_USER").unwrap_or_else(|_| "user".to_string()),
            bitcoin_rpc_pass: env::var("BITCOIN_RPC_PASS").unwrap_or_else(|_| "pass".to_string()),
            bitcoin_sync_interval: env::var("BITCOIN_SYNC_INTERVAL")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap_or(10),
            stacks_rpc_url: env::var("STACKS_RPC_URL")
                .unwrap_or_else(|_| "https://api.mainnet.hiro.so".to_string()),
            stacks_sync_interval: env::var("STACKS_SYNC_INTERVAL")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            api_port: env::var("API_PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .unwrap_or(3000),
            api_token: env::var("API_TOKEN")
                .unwrap_or_else(|_| "institutional-default-token".to_string()),
            ramp_api_key: env::var("RAMP_API_KEY")
                .unwrap_or_else(|_| "default-ramp-key".to_string()),
            investec_client_id: env::var("INVESTEC_CLIENT_ID")
                .unwrap_or_else(|_| "default-investec-id".to_string()),
            investec_secret: env::var("INVESTEC_SECRET")
                .unwrap_or_else(|_| "default-investec-secret".to_string()),
            alchemy_pay_app_id: env::var("ALCHEMY_PAY_APP_ID")
                .unwrap_or_else(|_| "default-alchemy-id".to_string()),
            alchemy_pay_secret: env::var("ALCHEMY_PAY_SECRET")
                .unwrap_or_else(|_| "default-alchemy-secret".to_string()),
            banxa_api_key: env::var("BANXA_API_KEY")
                .unwrap_or_else(|_| "default-banxa-key".to_string()),
            banxa_secret: env::var("BANXA_SECRET")
                .unwrap_or_else(|_| "default-banxa-secret".to_string()),
            infobip_api_key: env::var("INFOBIP_API_KEY")
                .unwrap_or_else(|_| "default-infobip-key".to_string()),
            infobip_base_url: env::var("INFOBIP_BASE_URL")
                .unwrap_or_else(|_| "https://api.infobip.com".to_string()),
            hmac_secret: env::var("HMAC_SECRET")
                .unwrap_or_else(|_| "default-hmac-secret".to_string()),
            fiat_webhook_secret,
            settlement_ingress_secret,
        }
    }
}
