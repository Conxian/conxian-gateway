use conxian_core::{ConxianError, ConxianResult};
use hmac::KeyInit;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::str::FromStr;
use tracing::info;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OnRampSessionRequest {
    pub wallet_address: String,
    pub amount: f64,
    pub currency: String,
    pub provider: String, // "ramp", "stitch", "ozow", "papss", "alchemypay", or "banxa"
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OnRampSessionResponse {
    pub session_id: String,
    pub redirect_url: String,
    pub provider: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebhookPayload {
    pub provider: String,
    pub event_type: String,
    pub reference_id: String,
    pub amount: f64,
    pub status: String,
    pub signature: String,
    pub raw_payload: String,
}

/// A fiat on-ramp provider. Kept as a closed set of named, first-class variants;
/// adding a provider is a new variant + an adapter, never a change to the router.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FiatOnRampProvider {
    Ramp,
    Stitch,
    Ozow,
    Papss,
    AlchemyPay,
    Banxa,
}

impl FiatOnRampProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ramp => "ramp",
            Self::Stitch => "stitch",
            Self::Ozow => "ozow",
            Self::Papss => "papss",
            Self::AlchemyPay => "alchemypay",
            Self::Banxa => "banxa",
        }
    }
}

impl FromStr for FiatOnRampProvider {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ramp" => Ok(Self::Ramp),
            "stitch" => Ok(Self::Stitch),
            "ozow" => Ok(Self::Ozow),
            "papss" => Ok(Self::Papss),
            "alchemypay" => Ok(Self::AlchemyPay),
            "banxa" => Ok(Self::Banxa),
            _ => Err(format!("unsupported fiat on-ramp provider: {s}")),
        }
    }
}

/// A fiat on-ramp connector. Each adapter owns the secret it embeds in its
/// redirect URL; providers that embed no secret (Stitch, Ozow, Banxa) hold nothing.
pub trait FiatOnRampAdapter: Send + Sync {
    fn provider(&self) -> FiatOnRampProvider;
    fn build_redirect_url(&self, request: &OnRampSessionRequest, session_id: &str) -> String;
}

pub struct RampAdapter {
    api_key: String,
}

impl RampAdapter {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

impl FiatOnRampAdapter for RampAdapter {
    fn provider(&self) -> FiatOnRampProvider {
        FiatOnRampProvider::Ramp
    }

    fn build_redirect_url(&self, request: &OnRampSessionRequest, _session_id: &str) -> String {
        format!(
            "https://buy.ramp.network/?userAddress={}&swapAmount={}&swapAsset={}&apiKey={}",
            request.wallet_address, request.amount, request.currency, self.api_key
        )
    }
}

/// Bank-agnostic South African on-ramp via Stitch: instant bank-to-bank (EFT),
/// card, and crypto-pay (settle crypto -> ZAR). Stitch issues the hosted
/// checkout link server-side, so no secret is embedded in the redirect URL.
pub struct StitchAdapter;

impl FiatOnRampAdapter for StitchAdapter {
    fn provider(&self) -> FiatOnRampProvider {
        FiatOnRampProvider::Stitch
    }

    fn build_redirect_url(&self, request: &OnRampSessionRequest, session_id: &str) -> String {
        format!(
            "https://checkout.stitch.money/?reference={}&amount={}&currency={}",
            session_id, request.amount, request.currency
        )
    }
}

/// Bank-agnostic South African instant-EFT on-ramp via Ozow (47M+ bank accounts).
/// No secret is embedded in the hosted redirect URL.
pub struct OzowAdapter;

impl FiatOnRampAdapter for OzowAdapter {
    fn provider(&self) -> FiatOnRampProvider {
        FiatOnRampProvider::Ozow
    }

    fn build_redirect_url(&self, request: &OnRampSessionRequest, session_id: &str) -> String {
        format!(
            "https://pay.ozow.com/?reference={}&amount={}",
            session_id, request.amount
        )
    }
}

/// Pan-African cross-border rail via PAPSS (Afreximbank): instant settlement
/// across African central banks in local currency. Modeled as a hosted
/// initiation like Stitch/Ozow; full ISO 20022 integration is a follow-up.
pub struct PapssAdapter;

impl FiatOnRampAdapter for PapssAdapter {
    fn provider(&self) -> FiatOnRampProvider {
        FiatOnRampProvider::Papss
    }

    fn build_redirect_url(&self, request: &OnRampSessionRequest, session_id: &str) -> String {
        format!(
            "https://papss.afreximbank.com/payments/initiate?reference={}&amount={}&currency={}",
            session_id, request.amount, request.currency
        )
    }
}

pub struct AlchemyPayAdapter {
    app_id: String,
}

impl AlchemyPayAdapter {
    pub fn new(app_id: String) -> Self {
        Self { app_id }
    }
}

impl FiatOnRampAdapter for AlchemyPayAdapter {
    fn provider(&self) -> FiatOnRampProvider {
        FiatOnRampProvider::AlchemyPay
    }

    fn build_redirect_url(&self, request: &OnRampSessionRequest, _session_id: &str) -> String {
        format!(
            "https://ramp.alchemypay.org/?address={}&cryptoAmount={}&crypto={}&appId={}",
            request.wallet_address, request.amount, request.currency, self.app_id
        )
    }
}

pub struct BanxaAdapter;

impl FiatOnRampAdapter for BanxaAdapter {
    fn provider(&self) -> FiatOnRampProvider {
        FiatOnRampProvider::Banxa
    }

    fn build_redirect_url(&self, request: &OnRampSessionRequest, _session_id: &str) -> String {
        format!(
            "https://conxian-labs.banxa.com/?walletAddress={}&coinAmount={}&coinType={}",
            request.wallet_address, request.amount, request.currency
        )
    }
}

pub struct FiatRouter {
    adapters: Vec<Box<dyn FiatOnRampAdapter>>,
}

impl FiatRouter {
    /// Build a router from the providers enabled for this lane. `None` means the
    /// provider is disabled and its route is unavailable (no secret required).
    pub fn from_enabled(
        ramp: Option<RampAdapter>,
        stitch: Option<StitchAdapter>,
        ozow: Option<OzowAdapter>,
        papss: Option<PapssAdapter>,
        alchemy_pay: Option<AlchemyPayAdapter>,
        banxa: Option<BanxaAdapter>,
    ) -> Self {
        let mut adapters: Vec<Box<dyn FiatOnRampAdapter>> = Vec::new();
        if let Some(adapter) = ramp {
            adapters.push(Box::new(adapter));
        }
        if let Some(adapter) = stitch {
            adapters.push(Box::new(adapter));
        }
        if let Some(adapter) = ozow {
            adapters.push(Box::new(adapter));
        }
        if let Some(adapter) = papss {
            adapters.push(Box::new(adapter));
        }
        if let Some(adapter) = alchemy_pay {
            adapters.push(Box::new(adapter));
        }
        if let Some(adapter) = banxa {
            adapters.push(Box::new(adapter));
        }
        Self { adapters }
    }

    pub async fn create_session(
        &self,
        request: OnRampSessionRequest,
    ) -> ConxianResult<OnRampSessionResponse> {
        let provider =
            FiatOnRampProvider::from_str(&request.provider).map_err(ConxianError::Api)?;

        let adapter = self
            .adapters
            .iter()
            .find(|adapter| adapter.provider() == provider)
            .ok_or_else(|| {
                ConxianError::Api(format!(
                    "fiat on-ramp provider not enabled: {}",
                    request.provider
                ))
            })?;

        info!(
            "Creating on-ramp session for {} via {}",
            request.wallet_address, request.provider
        );

        let session_id = format!("{}-{}", provider.as_str(), uuid::Uuid::new_v4());
        let redirect_url = adapter.build_redirect_url(&request, &session_id);

        Ok(OnRampSessionResponse {
            session_id,
            redirect_url,
            provider: provider.as_str().to_string(),
        })
    }

    pub fn verify_webhook(&self, payload: &WebhookPayload, secret: &str) -> ConxianResult<bool> {
        if secret.is_empty() {
            return Err(ConxianError::Security(
                "fiat webhook secret is not configured".to_string(),
            ));
        }

        if FiatOnRampProvider::from_str(&payload.provider).is_err() {
            return Err(ConxianError::Security(format!(
                "unknown webhook provider: {}",
                payload.provider
            )));
        }

        if payload.signature.is_empty() {
            return Ok(false);
        }

        info!(
            "Verifying {} webhook HMAC signature for reference: {}",
            payload.provider, payload.reference_id
        );

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|e| ConxianError::Security(format!("HMAC error: {}", e)))?;
        mac.update(payload.raw_payload.as_bytes());

        let sig_bytes = hex::decode(&payload.signature)
            .map_err(|e| ConxianError::Security(format!("Invalid signature hex: {}", e)))?;

        Ok(mac.verify_slice(&sig_bytes).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_router() -> FiatRouter {
        FiatRouter::from_enabled(
            Some(RampAdapter::new("test-key".to_string())),
            Some(StitchAdapter),
            Some(OzowAdapter),
            Some(PapssAdapter),
            Some(AlchemyPayAdapter::new("ap-app-id".to_string())),
            Some(BanxaAdapter),
        )
    }

    fn request(provider: &str) -> OnRampSessionRequest {
        OnRampSessionRequest {
            wallet_address: "bc1qtest".to_string(),
            amount: 100.0,
            currency: "BTC".to_string(),
            provider: provider.to_string(),
        }
    }

    #[tokio::test]
    async fn test_create_ramp_session() {
        let res = test_router().create_session(request("ramp")).await.unwrap();
        assert_eq!(res.provider, "ramp");
        assert!(res.redirect_url.contains("bc1qtest"));
        assert!(res.redirect_url.contains("test-key"));
    }

    #[tokio::test]
    async fn test_create_stitch_session() {
        let res = test_router()
            .create_session(request("stitch"))
            .await
            .unwrap();
        assert_eq!(res.provider, "stitch");
        assert!(res.redirect_url.contains("stitch.money"));
    }

    #[tokio::test]
    async fn test_create_ozow_session() {
        let res = test_router().create_session(request("ozow")).await.unwrap();
        assert_eq!(res.provider, "ozow");
        assert!(res.redirect_url.contains("ozow.com"));
    }

    #[tokio::test]
    async fn test_create_papss_session() {
        let res = test_router().create_session(request("papss")).await.unwrap();
        assert_eq!(res.provider, "papss");
        assert!(res.redirect_url.contains("papss.afreximbank.com"));
    }

    #[tokio::test]
    async fn test_create_alchemypay_session() {
        let res = test_router()
            .create_session(request("alchemypay"))
            .await
            .unwrap();
        assert_eq!(res.provider, "alchemypay");
        assert!(res.redirect_url.contains("bc1qtest"));
        assert!(res.redirect_url.contains("ap-app-id"));
    }

    #[tokio::test]
    async fn test_create_banxa_session() {
        let res = test_router()
            .create_session(request("banxa"))
            .await
            .unwrap();
        assert_eq!(res.provider, "banxa");
        assert!(res.redirect_url.contains("bc1qtest"));
    }

    #[tokio::test]
    async fn test_create_session_rejects_unknown_provider() {
        let res = test_router().create_session(request("moonpay")).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_create_session_rejects_disabled_provider() {
        let router = FiatRouter::from_enabled(None, None, None, None, None, None);
        let res = router.create_session(request("ramp")).await;
        assert!(res.is_err());
    }

    fn signed_webhook(provider: &str, secret: &str) -> (WebhookPayload, String) {
        let raw_payload = r#"{"reference":"ref123","status":"SUCCESS"}"#.to_string();
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(raw_payload.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        let payload = WebhookPayload {
            provider: provider.to_string(),
            event_type: "ORDER_CREATED".to_string(),
            reference_id: "ref123".to_string(),
            amount: 100.0,
            status: "SUCCESS".to_string(),
            signature,
            raw_payload,
        };
        (payload, secret.to_string())
    }

    #[tokio::test]
    async fn test_verify_ramp_webhook() {
        let (payload, secret) = signed_webhook("ramp", "webhook-secret");
        assert!(test_router().verify_webhook(&payload, &secret).unwrap());
    }

    #[tokio::test]
    async fn test_verify_banxa_webhook() {
        let (payload, secret) = signed_webhook("banxa", "banxa-secret");
        assert!(test_router().verify_webhook(&payload, &secret).unwrap());
    }

    #[tokio::test]
    async fn test_verify_stitch_webhook() {
        let (payload, secret) = signed_webhook("stitch", "stitch-secret");
        assert!(test_router().verify_webhook(&payload, &secret).unwrap());
    }

    #[tokio::test]
    async fn test_verify_webhook_rejects_wrong_secret() {
        let (payload, _) = signed_webhook("ramp", "webhook-secret");
        assert!(!test_router()
            .verify_webhook(&payload, "wrong-secret")
            .unwrap());
    }

    #[tokio::test]
    async fn test_verify_webhook_fails_closed_when_secret_missing() {
        let (payload, _) = signed_webhook("stitch", "stitch-secret");
        let result = test_router().verify_webhook(&payload, "");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verify_webhook_rejects_unknown_provider() {
        let (mut payload, _) = signed_webhook("ramp", "webhook-secret");
        payload.provider = "unknown".to_string();
        let result = test_router().verify_webhook(&payload, "webhook-secret");
        assert!(result.is_err());
    }
}
