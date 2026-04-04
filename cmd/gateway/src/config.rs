use std::env;

const FIAT_WEBHOOK_SECRET_SENTINEL: &str = "CHANGEME_FIAT_WEBHOOK_SECRET";
const SETTLEMENT_INGRESS_SECRET_SENTINEL: &str = "CHANGEME_SETTLEMENT_INGRESS_SECRET";

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
        let fiat_webhook_secret = env::var("FIAT_WEBHOOK_SECRET")
            .expect("FIAT_WEBHOOK_SECRET must be set (and should be a strong random secret)");
        let fiat_webhook_secret = fiat_webhook_secret.trim().to_string();
        let settlement_ingress_secret = env::var("SETTLEMENT_INGRESS_SECRET")
            .expect("SETTLEMENT_INGRESS_SECRET must be set (and should be a strong random secret)");
        let settlement_ingress_secret = settlement_ingress_secret.trim().to_string();

        if fiat_webhook_secret.is_empty() || fiat_webhook_secret == FIAT_WEBHOOK_SECRET_SENTINEL {
            panic!(
                "FIAT_WEBHOOK_SECRET must be a non-empty secret (not {FIAT_WEBHOOK_SECRET_SENTINEL})"
            );
        }

        if settlement_ingress_secret.is_empty()
            || settlement_ingress_secret == SETTLEMENT_INGRESS_SECRET_SENTINEL
        {
            panic!(
                "SETTLEMENT_INGRESS_SECRET must be a non-empty secret (not {SETTLEMENT_INGRESS_SECRET_SENTINEL})"
            );
        }

        if settlement_ingress_secret == fiat_webhook_secret {
            panic!("SETTLEMENT_INGRESS_SECRET must be distinct from FIAT_WEBHOOK_SECRET");
        }

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

    struct SecretsEnvRestore {
        old_fiat_webhook_secret: Option<String>,
        old_settlement_ingress_secret: Option<String>,
    }

    impl Drop for SecretsEnvRestore {
        fn drop(&mut self) {
            restore_env("FIAT_WEBHOOK_SECRET", self.old_fiat_webhook_secret.clone());
            restore_env(
                "SETTLEMENT_INGRESS_SECRET",
                self.old_settlement_ingress_secret.clone(),
            );
        }
    }

    #[test]
    fn from_env_trims_secret_whitespace() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

        let _env_restore = SecretsEnvRestore {
            old_fiat_webhook_secret: env::var("FIAT_WEBHOOK_SECRET").ok(),
            old_settlement_ingress_secret: env::var("SETTLEMENT_INGRESS_SECRET").ok(),
        };

        env::set_var("FIAT_WEBHOOK_SECRET", "  fiat-secret  ");
        env::set_var("SETTLEMENT_INGRESS_SECRET", "\tsettlement-secret\n");

        let config = Config::from_env();
        assert_eq!(config.fiat_webhook_secret, "fiat-secret");
        assert_eq!(config.settlement_ingress_secret, "settlement-secret");
    }

    #[test]
    fn from_env_checks_distinct_secrets_after_trimming() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

        let _env_restore = SecretsEnvRestore {
            old_fiat_webhook_secret: env::var("FIAT_WEBHOOK_SECRET").ok(),
            old_settlement_ingress_secret: env::var("SETTLEMENT_INGRESS_SECRET").ok(),
        };

        env::set_var("FIAT_WEBHOOK_SECRET", "shared-secret");
        env::set_var("SETTLEMENT_INGRESS_SECRET", " shared-secret ");

        let result = std::panic::catch_unwind(Config::from_env);
        assert!(result.is_err());

        let err = match result {
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
