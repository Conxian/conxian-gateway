use std::env;

const FIAT_WEBHOOK_SECRET_SENTINEL: &str = "sentinel_FIAT_WEBHOOK_SECRET";
const SETTLEMENT_INGRESS_SECRET_SENTINEL: &str = "sentinel_SETTLEMENT_INGRESS_SECRET";
const API_TOKEN_SENTINEL: &str = "sentinel_API_TOKEN";
const RAMP_API_KEY_SENTINEL: &str = "sentinel_RAMP_API_KEY";
const INVESTEC_CLIENT_ID_SENTINEL: &str = "sentinel_INVESTEC_CLIENT_ID";
const INVESTEC_SECRET_SENTINEL: &str = "sentinel_INVESTEC_SECRET";
const ALCHEMY_PAY_APP_ID_SENTINEL: &str = "sentinel_ALCHEMY_PAY_APP_ID";
const ALCHEMY_PAY_SECRET_SENTINEL: &str = "sentinel_ALCHEMY_PAY_SECRET";
const BANXA_API_KEY_SENTINEL: &str = "sentinel_BANXA_API_KEY";
const BANXA_SECRET_SENTINEL: &str = "sentinel_BANXA_SECRET";
const INFOBIP_API_KEY_SENTINEL: &str = "sentinel_INFOBIP_API_KEY";
const HMAC_SECRET_SENTINEL: &str = "sentinel_HMAC_SECRET";
const ORACLE_PUBKEY_SENTINEL: &str = "sentinel_ORACLE_PUBKEY";
const OFFLINE_QUEUE_SECRET_SENTINEL: &str = "sentinel_OFFLINE_QUEUE_SECRET";

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Mainnet,
    Testnet,
    Simulated,
}

impl std::fmt::Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mainnet => write!(f, "mainnet"),
            Self::Testnet => write!(f, "testnet"),
            Self::Simulated => write!(f, "simulated"),
        }
    }
}

impl Network {
    pub fn from_env() -> Self {
        match env::var("CONXIAN_NETWORK")
            .unwrap_or_else(|_| "mainnet".to_string())
            .to_lowercase()
            .as_str()
        {
            "mainnet" => Self::Mainnet,
            "testnet" => Self::Testnet,
            "simulated" => Self::Simulated,
            _ => Self::Mainnet,
        }
    }
}

#[allow(dead_code)]
pub struct Config {
    pub bitcoin_rpc_url: String,
    pub bitcoin_rpc_user: String,
    pub bitcoin_rpc_pass: String,
    pub bitcoin_sync_interval: u64,
    pub mempool_orchestrator_interval: u64,
    pub mempool_stuck_threshold_secs: u64,
    pub mempool_max_fee_bump_attempts: u32,
    pub mempool_max_fee_rate_sat_vb: u64,
    pub mempool_min_bump_increment_sat_vb: u64,
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
    pub alex_api_url: String,
    pub oracle_pubkey: String,
    pub offline_queue_secret: String,
    pub rgb_mode: conxian_core::RolloutMode,
    pub rgb_node_url: String,
    pub network: Network,
    pub liquid_rpc_url: String,
    pub rootstock_rpc_url: String,
}

impl Config {
    fn get_mandatory_env(key: &str, sentinel: &str) -> String {
        let val = env::var(key).unwrap_or_else(|_| panic!("{} must be set", key));
        let val = val.trim().to_string();
        if val.is_empty() || val == sentinel {
            panic!("{} must be a non-empty secret (not {})", key, sentinel);
        }
        val
    }

    pub fn from_env() -> Self {
        let fiat_webhook_secret =
            Self::get_mandatory_env("FIAT_WEBHOOK_SECRET", FIAT_WEBHOOK_SECRET_SENTINEL);
        let settlement_ingress_secret = Self::get_mandatory_env(
            "SETTLEMENT_INGRESS_SECRET",
            SETTLEMENT_INGRESS_SECRET_SENTINEL,
        );
        let api_token = Self::get_mandatory_env("API_TOKEN", API_TOKEN_SENTINEL);
        let ramp_api_key = Self::get_mandatory_env("RAMP_API_KEY", RAMP_API_KEY_SENTINEL);
        let investec_client_id =
            Self::get_mandatory_env("INVESTEC_CLIENT_ID", INVESTEC_CLIENT_ID_SENTINEL);
        let investec_secret = Self::get_mandatory_env("INVESTEC_SECRET", INVESTEC_SECRET_SENTINEL);
        let alchemy_pay_app_id =
            Self::get_mandatory_env("ALCHEMY_PAY_APP_ID", ALCHEMY_PAY_APP_ID_SENTINEL);
        let alchemy_pay_secret =
            Self::get_mandatory_env("ALCHEMY_PAY_SECRET", ALCHEMY_PAY_SECRET_SENTINEL);
        let banxa_api_key = Self::get_mandatory_env("BANXA_API_KEY", BANXA_API_KEY_SENTINEL);
        let banxa_secret = Self::get_mandatory_env("BANXA_SECRET", BANXA_SECRET_SENTINEL);
        let infobip_api_key = Self::get_mandatory_env("INFOBIP_API_KEY", INFOBIP_API_KEY_SENTINEL);
        let hmac_secret = Self::get_mandatory_env("HMAC_SECRET", HMAC_SECRET_SENTINEL);
        let oracle_pubkey = Self::get_mandatory_env("ORACLE_PUBKEY", ORACLE_PUBKEY_SENTINEL);
        let offline_queue_secret =
            Self::get_mandatory_env("OFFLINE_QUEUE_SECRET", OFFLINE_QUEUE_SECRET_SENTINEL);
        let rgb_mode = match env::var("RGB_MODE")
            .unwrap_or_else(|_| "disabled".to_string())
            .to_lowercase()
            .as_str()
        {
            "active" => conxian_core::RolloutMode::Active,
            "shadow" => conxian_core::RolloutMode::Shadow,
            _ => conxian_core::RolloutMode::Disabled,
        };
        let network = Network::from_env();
        let liquid_rpc_url =
            env::var("LIQUID_RPC_URL").unwrap_or_else(|_| "http://localhost:18843".to_string());
        let rootstock_rpc_url =
            env::var("ROOTSTOCK_RPC_URL").unwrap_or_else(|_| "http://localhost:4444".to_string());

        let (btc_url, stx_url, alex_url) = match network {
            Network::Mainnet => (
                "https://bitcoin-rpc.publicnode.com".to_string(),
                "https://api.mainnet.hiro.so".to_string(),
                "https://api.alexlab.co".to_string(),
            ),
            Network::Testnet => (
                "https://bitcoin-testnet.publicnode.com".to_string(),
                "https://api.testnet.hiro.so".to_string(),
                "https://api.testnet.alexlab.co".to_string(),
            ),
            Network::Simulated => (
                "http://localhost:18443".to_string(),
                "http://localhost:3999".to_string(),
                "http://localhost:3010".to_string(),
            ),
        };

        if settlement_ingress_secret == fiat_webhook_secret {
            panic!("SETTLEMENT_INGRESS_SECRET must be distinct from FIAT_WEBHOOK_SECRET");
        }

        Self {
            bitcoin_rpc_url: env::var("BITCOIN_RPC_URL").unwrap_or(btc_url),
            bitcoin_rpc_user: env::var("BITCOIN_RPC_USER").unwrap_or_default(),
            bitcoin_rpc_pass: env::var("BITCOIN_RPC_PASS").unwrap_or_default(),
            bitcoin_sync_interval: env::var("BITCOIN_SYNC_INTERVAL")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap_or(10),
            mempool_orchestrator_interval: env::var("MEMPOOL_ORCHESTRATOR_INTERVAL")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            mempool_stuck_threshold_secs: env::var("MEMPOOL_STUCK_THRESHOLD_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .unwrap_or(300),
            mempool_max_fee_bump_attempts: env::var("MEMPOOL_MAX_FEE_BUMP_ATTEMPTS")
                .unwrap_or_else(|_| "3".to_string())
                .parse()
                .unwrap_or(3),
            mempool_max_fee_rate_sat_vb: env::var("MEMPOOL_MAX_FEE_RATE_SAT_VB")
                .unwrap_or_else(|_| "150".to_string())
                .parse()
                .unwrap_or(150),
            mempool_min_bump_increment_sat_vb: env::var("MEMPOOL_MIN_BUMP_INCREMENT_SAT_VB")
                .unwrap_or_else(|_| "2".to_string())
                .parse()
                .unwrap_or(2),
            stacks_rpc_url: env::var("STACKS_RPC_URL").unwrap_or(stx_url),
            stacks_sync_interval: env::var("STACKS_SYNC_INTERVAL")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            api_port: env::var("API_PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .unwrap_or(3000),
            api_token,
            ramp_api_key,
            investec_client_id,
            investec_secret,
            alchemy_pay_app_id,
            alchemy_pay_secret,
            banxa_api_key,
            banxa_secret,
            infobip_api_key,
            infobip_base_url: env::var("INFOBIP_BASE_URL")
                .unwrap_or_else(|_| "https://api.infobip.com".to_string()),
            hmac_secret,
            fiat_webhook_secret,
            settlement_ingress_secret,
            oracle_pubkey,
            offline_queue_secret,
            rgb_mode,
            rgb_node_url: env::var("RGB_NODE_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string()),
            network,
            alex_api_url: env::var("ALEX_API_URL").unwrap_or(alex_url),
            liquid_rpc_url,
            rootstock_rpc_url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn restore_env(key: &str, value: Option<String>) {
        match value {
            Some(value) => env::set_var(key, value),
            None => env::remove_var(key),
        }
    }

    struct FullEnvRestore {
        vars: Vec<(&'static str, Option<String>)>,
    }

    impl FullEnvRestore {
        fn new() -> Self {
            let keys = vec![
                "FIAT_WEBHOOK_SECRET",
                "SETTLEMENT_INGRESS_SECRET",
                "API_TOKEN",
                "RAMP_API_KEY",
                "INVESTEC_CLIENT_ID",
                "INVESTEC_SECRET",
                "ALCHEMY_PAY_APP_ID",
                "ALCHEMY_PAY_SECRET",
                "BANXA_API_KEY",
                "BANXA_SECRET",
                "INFOBIP_API_KEY",
                "HMAC_SECRET",
                "OFFLINE_QUEUE_SECRET",
                "MEMPOOL_ORCHESTRATOR_INTERVAL",
                "MEMPOOL_STUCK_THRESHOLD_SECS",
                "MEMPOOL_MAX_FEE_BUMP_ATTEMPTS",
                "MEMPOOL_MAX_FEE_RATE_SAT_VB",
                "MEMPOOL_MIN_BUMP_INCREMENT_SAT_VB",
            ];
            let vars = keys.into_iter().map(|k| (k, env::var(k).ok())).collect();
            Self { vars }
        }
    }

    impl Drop for FullEnvRestore {
        fn drop(&mut self) {
            for (key, value) in &self.vars {
                restore_env(key, value.clone());
            }
        }
    }

    fn set_test_envs() {
        env::set_var("FIAT_WEBHOOK_SECRET", "fiat-secret");
        env::set_var("SETTLEMENT_INGRESS_SECRET", "settlement-secret");
        env::set_var("API_TOKEN", "api-token");
        env::set_var("RAMP_API_KEY", "ramp-key");
        env::set_var("INVESTEC_CLIENT_ID", "investec-id");
        env::set_var("INVESTEC_SECRET", "investec-secret");
        env::set_var("ALCHEMY_PAY_APP_ID", "alchemy-id");
        env::set_var("ALCHEMY_PAY_SECRET", "alchemy-secret");
        env::set_var("BANXA_API_KEY", "banxa-key");
        env::set_var("BANXA_SECRET", "banxa-secret");
        env::set_var("INFOBIP_API_KEY", "infobip-key");
        env::set_var("HMAC_SECRET", "hmac-secret");
        env::set_var("ORACLE_PUBKEY", "oracle-key");
        env::set_var(
            "OFFLINE_QUEUE_SECRET",
            "offline-queue-secret-that-is-at-least-32-bytes-long-for-prod",
        );
    }

    #[test]
    fn from_env_trims_secret_whitespace() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let _env_restore = FullEnvRestore::new();
        set_test_envs();

        env::set_var("FIAT_WEBHOOK_SECRET", "  fiat-secret  ");
        env::set_var("SETTLEMENT_INGRESS_SECRET", "\tsettlement-secret\n");

        let config = Config::from_env();
        assert_eq!(config.fiat_webhook_secret, "fiat-secret");
        assert_eq!(config.settlement_ingress_secret, "settlement-secret");
    }

    #[test]
    fn from_env_checks_distinct_secrets_after_trimming() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let _env_restore = FullEnvRestore::new();
        set_test_envs();

        env::set_var("FIAT_WEBHOOK_SECRET", "shared-secret");
        env::set_var("SETTLEMENT_INGRESS_SECRET", " shared-secret ");

        let err = match std::panic::catch_unwind(Config::from_env) {
            Ok(_) => panic!("expected Config::from_env to panic"),
            Err(err) => err,
        };
        let message = err
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        assert!(message.contains("must be distinct"));
    }
}
