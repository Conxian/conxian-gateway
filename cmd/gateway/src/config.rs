use std::env;

const FIAT_WEBHOOK_SECRET_SENTINEL: &str = "CHANGEME_FIAT_WEBHOOK_SECRET";
const SETTLEMENT_INGRESS_SECRET_SENTINEL: &str = "CHANGEME_SETTLEMENT_INGRESS_SECRET";
const API_TOKEN_SENTINEL: &str = "CHANGEME_API_TOKEN";
const RAMP_API_KEY_SENTINEL: &str = "CHANGEME_RAMP_API_KEY";
const INVESTEC_CLIENT_ID_SENTINEL: &str = "CHANGEME_INVESTEC_CLIENT_ID";
const INVESTEC_SECRET_SENTINEL: &str = "CHANGEME_INVESTEC_SECRET";
const ALCHEMY_PAY_APP_ID_SENTINEL: &str = "CHANGEME_ALCHEMY_PAY_APP_ID";
const ALCHEMY_PAY_SECRET_SENTINEL: &str = "CHANGEME_ALCHEMY_PAY_SECRET";
const BANXA_API_KEY_SENTINEL: &str = "CHANGEME_BANXA_API_KEY";
const BANXA_SECRET_SENTINEL: &str = "CHANGEME_BANXA_SECRET";
const INFOBIP_API_KEY_SENTINEL: &str = "CHANGEME_INFOBIP_API_KEY";
const HMAC_SECRET_SENTINEL: &str = "CHANGEME_HMAC_SECRET";

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

        if settlement_ingress_secret == fiat_webhook_secret {
            panic!("SETTLEMENT_INGRESS_SECRET must be distinct from FIAT_WEBHOOK_SECRET");
        }

        Self {
            bitcoin_rpc_url: env::var("BITCOIN_RPC_URL")
                .unwrap_or_else(|_| "https://bitcoin-rpc.publicnode.com".to_string()),
            bitcoin_rpc_user: env::var("BITCOIN_RPC_USER").unwrap_or_default(),
            bitcoin_rpc_pass: env::var("BITCOIN_RPC_PASS").unwrap_or_default(),
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
