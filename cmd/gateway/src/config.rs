use std::net::IpAddr;
use std::path::Path;
use std::{env, panic};

use url::Url;

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
const OFFLINE_QUEUE_SECRET_SENTINEL: &str = "sentinel_OFFLINE_QUEUE_SECRET";
const REDIS_URL_SENTINEL: &str = "sentinel_REDIS_URL";
const REDIS_USERNAME_SENTINEL: &str = "sentinel_REDIS_USERNAME";
const REDIS_PASSWORD_SENTINEL: &str = "sentinel_REDIS_PASSWORD";
const TOKEN_TTL_SENTINEL: &str = "sentinel_TOKEN_TTL";
const EXCLUSIVE_LOCAL_WRITER_MODE: &str = "exclusive-local-writer";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilesystemClass {
    Local(&'static str),
    SharedOrNetwork(&'static str),
    Unknown(u32),
}

fn classify_filesystem_magic(magic: u32) -> FilesystemClass {
    match magic {
        0x0000_ef53 => FilesystemClass::Local("ext"),
        0x5846_5342 => FilesystemClass::Local("xfs"),
        0x9123_683e => FilesystemClass::Local("btrfs"),
        0xf2f5_2010 => FilesystemClass::Local("f2fs"),
        0x0102_1994 => FilesystemClass::Local("tmpfs"),
        0x794c_7630 => FilesystemClass::Local("overlayfs"),
        0x8584_58f6 => FilesystemClass::Local("ramfs"),
        0x2fc1_2fc1 => FilesystemClass::Local("zfs"),
        0x3153_464a => FilesystemClass::Local("jfs"),
        0x5265_4973 => FilesystemClass::Local("reiserfs"),
        0x0000_3434 => FilesystemClass::Local("nilfs"),
        0x2405_1905 => FilesystemClass::Local("ubifs"),
        0x0000_6969 => FilesystemClass::SharedOrNetwork("nfs"),
        0x0000_517b => FilesystemClass::SharedOrNetwork("smb"),
        0xff53_4d42 => FilesystemClass::SharedOrNetwork("cifs"),
        0x00c3_6400 => FilesystemClass::SharedOrNetwork("ceph"),
        0x0102_1997 => FilesystemClass::SharedOrNetwork("9p"),
        0x7375_7245 => FilesystemClass::SharedOrNetwork("coda"),
        0x5346_414f => FilesystemClass::SharedOrNetwork("afs"),
        0x0000_564c => FilesystemClass::SharedOrNetwork("ncp"),
        0x0116_1970 => FilesystemClass::SharedOrNetwork("gfs2"),
        0x0bd0_0bd0 => FilesystemClass::SharedOrNetwork("lustre"),
        0x4750_4653 => FilesystemClass::SharedOrNetwork("gpfs"),
        other => FilesystemClass::Unknown(other),
    }
}

fn classify_filesystem_type(raw_type: i64) -> FilesystemClass {
    classify_filesystem_magic(raw_type as u32)
}

fn validate_filesystem_policy(class: FilesystemClass, allow_unknown: bool) -> Result<(), String> {
    match class {
        FilesystemClass::Local(_) => Ok(()),
        FilesystemClass::SharedOrNetwork(name) => Err(format!(
            "Gateway file persistence rejects known shared/network filesystem '{name}'"
        )),
        FilesystemClass::Unknown(_) if allow_unknown => Ok(()),
        FilesystemClass::Unknown(magic) => Err(format!(
            "Gateway file persistence cannot verify filesystem type 0x{magic:08x} as local; set GATEWAY_ALLOW_UNKNOWN_STATE_FILESYSTEM=true only after operator review"
        )),
    }
}

pub fn validate_state_filesystem(state_path: &Path, allow_unknown: bool) -> Result<(), String> {
    let parent = state_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical_parent = parent.canonicalize().map_err(|error| {
        format!(
            "failed to resolve Gateway state parent '{}': {error}",
            parent.display()
        )
    })?;

    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let path = CString::new(canonical_parent.as_os_str().as_bytes()).map_err(|_| {
            format!(
                "Gateway state parent '{}' contains an interior NUL byte",
                canonical_parent.display()
            )
        })?;
        let mut stats = std::mem::MaybeUninit::<libc::statfs>::uninit();
        // SAFETY: `path` is a valid NUL-terminated pathname and `stats` points
        // to writable storage for one `statfs` result.
        let result = unsafe { libc::statfs(path.as_ptr(), stats.as_mut_ptr()) };
        if result != 0 {
            return Err(format!(
                "failed to classify Gateway state parent '{}': {}",
                canonical_parent.display(),
                std::io::Error::last_os_error()
            ));
        }
        // Normalize through u32 so sign-extended Linux magic values classify
        // identically on every supported Linux architecture.
        let raw_type = unsafe { stats.assume_init() }.f_type as u32;
        validate_filesystem_policy(classify_filesystem_type(i64::from(raw_type)), allow_unknown)
    }

    #[cfg(not(target_os = "linux"))]
    {
        validate_filesystem_policy(FilesystemClass::Unknown(0), allow_unknown).map_err(|error| {
            format!(
                "{error}; filesystem classification is not implemented for {}",
                std::env::consts::OS
            )
        })
    }
}

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

#[derive(Debug, Clone)]
pub struct Config {
    pub gateway_state_path: String,
    pub gateway_allow_unknown_state_filesystem: bool,
    pub bitcoin_rpc_url: String,
    pub bitcoin_rpc_user: String,
    pub bitcoin_rpc_pass: String,
    pub bitcoin_core_shadow_observation_enabled: bool,
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
    pub offline_queue_secret: String,
    pub rgb_mode: conxian_core::RolloutMode,
    pub rgb_node_url: String,
    #[allow(dead_code)]
    pub rgb_stash_path: Option<String>,
    #[allow(dead_code)]
    pub rgb_esplora_url: Option<String>,
    pub network: Network,
    pub alex_api_url: String,
    pub alex_venue_manifest_path: Option<String>,
    pub liquid_rpc_url: String,
    pub rootstock_rpc_url: String,
    pub babylon_api_url: Option<String>,
    pub redis_url: Option<String>,
    pub redis_username: Option<String>,
    pub redis_password: Option<String>,
    pub token_ttl_seconds: Option<u64>,
}

impl Config {
    fn strict_truthy_env(key: &str) -> bool {
        env::var(key)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
    }

    fn get_mandatory_env(key: &str, sentinel: &str) -> String {
        let val = env::var(key).unwrap_or_else(|_| panic!("{} must be set", key));
        let val = val.trim().to_string();
        if val.is_empty() || val == sentinel {
            panic!("{} must be a non-empty secret (not {})", key, sentinel);
        }
        val
    }

    fn optional_env(key: &str) -> Option<String> {
        env::var(key).ok().and_then(|value| {
            let value = value.trim().to_string();
            (!value.is_empty()).then_some(value)
        })
    }

    fn validate_rgb_endpoint(key: &str, raw: &str) -> String {
        let url = Url::parse(raw).unwrap_or_else(|_| panic!("{} must be a valid URL", key));
        if url.scheme() != "http" && url.scheme() != "https" {
            panic!("{} must use http or https", key);
        }
        if !url.username().is_empty() || url.password().is_some() {
            panic!("{} must not contain embedded credentials", key);
        }
        let host = url
            .host_str()
            .unwrap_or_else(|| panic!("{} must include a host", key));
        let local_host = host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .map(|address| address.is_loopback())
                .unwrap_or(false);
        if url.scheme() == "http" && !local_host {
            panic!("{} may use plain HTTP only for local development", key);
        }
        raw.to_string()
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
        let offline_queue_secret =
            Self::get_mandatory_env("OFFLINE_QUEUE_SECRET", OFFLINE_QUEUE_SECRET_SENTINEL);
        let redis_url = env::var("REDIS_URL").ok().and_then(|val| {
            let val = val.trim().to_string();
            if val.is_empty() || val == REDIS_URL_SENTINEL {
                None
            } else {
                Some(val)
            }
        });
        let redis_username = env::var("REDIS_USERNAME").ok().and_then(|val| {
            let val = val.trim().to_string();
            if val.is_empty() || val == REDIS_USERNAME_SENTINEL {
                None
            } else {
                Some(val)
            }
        });
        let redis_password = env::var("REDIS_PASSWORD").ok().and_then(|val| {
            let val = val.trim().to_string();
            if val.is_empty() || val == REDIS_PASSWORD_SENTINEL {
                None
            } else {
                Some(val)
            }
        });
        let token_ttl_seconds = env::var("TOKEN_TTL_SECONDS").ok().and_then(|val| {
            let val = val.trim().to_string();
            if val.is_empty() || val == TOKEN_TTL_SENTINEL {
                None
            } else {
                val.parse().ok()
            }
        });

        let rgb_mode = match env::var("RGB_MODE")
            .unwrap_or_else(|_| "disabled".to_string())
            .to_lowercase()
            .as_str()
        {
            "active" => conxian_core::RolloutMode::Active,
            "shadow" => conxian_core::RolloutMode::Shadow,
            _ => conxian_core::RolloutMode::Disabled,
        };
        let rgb_node_url = Self::validate_rgb_endpoint(
            "RGB_NODE_URL",
            &env::var("RGB_NODE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string()),
        );
        let rgb_stash_path = Self::optional_env("RGB_STASH_PATH");
        let rgb_esplora_url = Self::optional_env("RGB_ESPLORA_URL")
            .map(|url| Self::validate_rgb_endpoint("RGB_ESPLORA_URL", &url));
        if rgb_stash_path.is_some() != rgb_esplora_url.is_some() {
            panic!("RGB_STASH_PATH and RGB_ESPLORA_URL must be configured together");
        }
        #[cfg(feature = "rgb-native")]
        if matches!(rgb_mode, conxian_core::RolloutMode::Active)
            && (rgb_stash_path.is_none() || rgb_esplora_url.is_none())
        {
            panic!(
                "RGB_STASH_PATH and RGB_ESPLORA_URL are required for Active mode with rgb-native"
            );
        }
        let network = Network::from_env();
        let liquid_rpc_url =
            env::var("LIQUID_RPC_URL").unwrap_or_else(|_| "http://localhost:18843".to_string());
        let rootstock_rpc_url =
            env::var("ROOTSTOCK_RPC_URL").unwrap_or_else(|_| "http://localhost:4444".to_string());
        let babylon_api_url = env::var("BABYLON_API_URL").ok().and_then(|value| {
            let value = value.trim().to_string();
            if value.is_empty() {
                None
            } else {
                Some(value)
            }
        });
        let alex_venue_manifest_path = Self::optional_env("ALEX_VENUE_MANIFEST_PATH");

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

        let gateway_state_path = env::var("GATEWAY_STATE_PATH")
            .unwrap_or_else(|_| "gateway_state.json".to_string())
            .trim()
            .to_string();
        if gateway_state_path.is_empty() {
            panic!("GATEWAY_STATE_PATH must be a non-empty file path");
        }
        let persistence_mode = env::var("GATEWAY_PERSISTENCE_MODE")
            .unwrap_or_else(|_| EXCLUSIVE_LOCAL_WRITER_MODE.to_string())
            .trim()
            .to_ascii_lowercase();
        if persistence_mode != EXCLUSIVE_LOCAL_WRITER_MODE {
            panic!(
                "GATEWAY_PERSISTENCE_MODE must be '{EXCLUSIVE_LOCAL_WRITER_MODE}'; active-active and shared-writer file persistence are unsupported"
            );
        }
        let gateway_allow_unknown_state_filesystem =
            match env::var("GATEWAY_ALLOW_UNKNOWN_STATE_FILESYSTEM") {
                Ok(value) if value.trim().eq_ignore_ascii_case("true") => true,
                Ok(value) if value.trim().eq_ignore_ascii_case("false") => false,
                Ok(_) => panic!("GATEWAY_ALLOW_UNKNOWN_STATE_FILESYSTEM must be 'true' or 'false'"),
                Err(_) => false,
            };

        Self {
            gateway_state_path,
            gateway_allow_unknown_state_filesystem,
            bitcoin_rpc_url: env::var("BITCOIN_RPC_URL").unwrap_or(btc_url),
            bitcoin_rpc_user: env::var("BITCOIN_RPC_USER").unwrap_or_default(),
            bitcoin_rpc_pass: env::var("BITCOIN_RPC_PASS").unwrap_or_default(),
            bitcoin_core_shadow_observation_enabled: Self::strict_truthy_env(
                "BITCOIN_CORE_SHADOW_OBSERVATION_ENABLED",
            ),
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
            offline_queue_secret,
            rgb_mode,
            rgb_node_url,
            rgb_stash_path,
            rgb_esplora_url,
            network,
            alex_api_url: env::var("ALEX_API_URL").unwrap_or(alex_url),
            alex_venue_manifest_path,
            liquid_rpc_url,
            rootstock_rpc_url,
            babylon_api_url,
            redis_url,
            redis_username,
            redis_password,
            token_ttl_seconds,
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
                "RGB_MODE",
                "RGB_NODE_URL",
                "RGB_STASH_PATH",
                "RGB_ESPLORA_URL",
                "MEMPOOL_ORCHESTRATOR_INTERVAL",
                "MEMPOOL_STUCK_THRESHOLD_SECS",
                "MEMPOOL_MAX_FEE_BUMP_ATTEMPTS",
                "MEMPOOL_MAX_FEE_RATE_SAT_VB",
                "MEMPOOL_MIN_BUMP_INCREMENT_SAT_VB",
                "REDIS_URL",
                "REDIS_USERNAME",
                "REDIS_PASSWORD",
                "TOKEN_TTL_SECONDS",
                "BABYLON_API_URL",
                "ALEX_VENUE_MANIFEST_PATH",
                "GATEWAY_STATE_PATH",
                "GATEWAY_PERSISTENCE_MODE",
                "GATEWAY_ALLOW_UNKNOWN_STATE_FILESYSTEM",
                "BITCOIN_CORE_SHADOW_OBSERVATION_ENABLED",
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
        env::set_var(
            "OFFLINE_QUEUE_SECRET",
            "offline-queue-secret-that-is-at-least-32-bytes-long-for-prod",
        );
        env::set_var("RGB_MODE", "disabled");
        env::remove_var("RGB_NODE_URL");
        env::remove_var("RGB_STASH_PATH");
        env::remove_var("RGB_ESPLORA_URL");
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

    #[test]
    fn from_env_reads_optional_babylon_api_url() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let _env_restore = FullEnvRestore::new();
        set_test_envs();

        env::set_var("BABYLON_API_URL", "  https://babylon.example  ");
        let config = Config::from_env();
        assert_eq!(
            config.babylon_api_url.as_deref(),
            Some("https://babylon.example")
        );

        env::set_var("BABYLON_API_URL", "   ");
        let config = Config::from_env();
        assert_eq!(config.babylon_api_url, None);
    }

    #[test]
    fn from_env_reads_optional_alex_manifest_path_without_defaulting_it() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let _env_restore = FullEnvRestore::new();
        set_test_envs();

        env::set_var("ALEX_VENUE_MANIFEST_PATH", "  /run/conxian/alex.json  ");
        let config = Config::from_env();
        assert_eq!(
            config.alex_venue_manifest_path.as_deref(),
            Some("/run/conxian/alex.json")
        );

        env::set_var("ALEX_VENUE_MANIFEST_PATH", "   ");
        let config = Config::from_env();
        assert_eq!(config.alex_venue_manifest_path, None);
    }

    #[test]
    fn from_env_reads_explicit_gateway_state_path() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let _env_restore = FullEnvRestore::new();
        set_test_envs();

        env::set_var(
            "GATEWAY_STATE_PATH",
            "  /var/lib/conxian/gateway-state.json  ",
        );
        let config = Config::from_env();

        assert_eq!(
            config.gateway_state_path,
            "/var/lib/conxian/gateway-state.json"
        );
        assert!(!config.gateway_allow_unknown_state_filesystem);
    }

    #[test]
    fn persistence_mode_rejects_shared_writer_variants() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let _env_restore = FullEnvRestore::new();
        set_test_envs();

        for mode in ["active-active", "shared-writer", "unknown-mode"] {
            env::set_var("GATEWAY_PERSISTENCE_MODE", mode);
            assert!(std::panic::catch_unwind(Config::from_env).is_err());
        }
    }

    #[test]
    fn shadow_observation_is_disabled_by_default_and_uses_strict_truthy_values() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let _env_restore = FullEnvRestore::new();
        set_test_envs();

        env::remove_var("BITCOIN_CORE_SHADOW_OBSERVATION_ENABLED");
        assert!(!Config::from_env().bitcoin_core_shadow_observation_enabled);

        for enabled in ["1", "true", "TRUE", " yes ", "On"] {
            env::set_var("BITCOIN_CORE_SHADOW_OBSERVATION_ENABLED", enabled);
            assert!(Config::from_env().bitcoin_core_shadow_observation_enabled);
        }

        for disabled in ["", " ", "0", "false", "enabled", "2", "truthy"] {
            env::set_var("BITCOIN_CORE_SHADOW_OBSERVATION_ENABLED", disabled);
            assert!(!Config::from_env().bitcoin_core_shadow_observation_enabled);
        }
    }

    #[test]
    fn persistence_mode_defaults_to_exclusive_local_writer() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let _env_restore = FullEnvRestore::new();
        set_test_envs();
        env::remove_var("GATEWAY_PERSISTENCE_MODE");

        let config = Config::from_env();
        assert!(!config.gateway_allow_unknown_state_filesystem);
    }

    #[test]
    fn unknown_filesystem_override_requires_explicit_boolean() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let _env_restore = FullEnvRestore::new();
        set_test_envs();

        env::set_var("GATEWAY_ALLOW_UNKNOWN_STATE_FILESYSTEM", "true");
        assert!(Config::from_env().gateway_allow_unknown_state_filesystem);

        env::set_var("GATEWAY_ALLOW_UNKNOWN_STATE_FILESYSTEM", "yes");
        assert!(std::panic::catch_unwind(Config::from_env).is_err());
    }

    #[test]
    fn filesystem_classifier_normalizes_sign_extended_magic_values() {
        let sign_extended = |magic: u32| i32::from_ne_bytes(magic.to_ne_bytes()) as i64;

        assert_eq!(
            classify_filesystem_type(sign_extended(0xff53_4d42)),
            FilesystemClass::SharedOrNetwork("cifs")
        );
        assert_eq!(
            classify_filesystem_type(sign_extended(0xf2f5_2010)),
            FilesystemClass::Local("f2fs")
        );
        assert_eq!(
            classify_filesystem_type(sign_extended(0x9123_683e)),
            FilesystemClass::Local("btrfs")
        );
    }

    #[test]
    fn unknown_override_never_bypasses_known_shared_filesystems() {
        assert!(validate_filesystem_policy(FilesystemClass::Unknown(7), false).is_err());
        assert!(validate_filesystem_policy(FilesystemClass::Unknown(7), true).is_ok());
        assert!(validate_filesystem_policy(FilesystemClass::SharedOrNetwork("nfs"), true).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn current_linux_test_filesystem_is_accepted_as_known_local() {
        let state_path = env::current_dir().unwrap().join("gateway-state-test.json");
        validate_state_filesystem(&state_path, false).unwrap();
    }

    #[test]
    fn rgb_endpoints_reject_credentials_and_remote_plain_http() {
        let credentials = std::panic::catch_unwind(|| {
            Config::validate_rgb_endpoint("RGB_NODE_URL", "https://user:pass@example.com")
        });
        assert!(credentials.is_err());

        let remote_http = std::panic::catch_unwind(|| {
            Config::validate_rgb_endpoint("RGB_ESPLORA_URL", "http://example.com/api")
        });
        assert!(remote_http.is_err());
    }

    #[cfg(feature = "rgb-native")]
    #[test]
    fn active_native_mode_requires_stash_configuration() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let _env_restore = FullEnvRestore::new();
        set_test_envs();
        env::set_var("RGB_MODE", "active");

        let err =
            std::panic::catch_unwind(Config::from_env).expect_err("expected RGB config panic");
        let message = err
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        assert!(message.contains("RGB_STASH_PATH"));
    }
}
