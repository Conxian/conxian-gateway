use async_trait::async_trait;
use base64::Engine as _;
use conxian_core::{ConxianError, ConxianResult};
use hmac::KeyInit;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::str::FromStr;
use subtle::ConstantTimeEq;
use tracing::info;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OnRampSessionRequest {
    pub wallet_address: String,
    pub amount: f64,
    pub currency: String,
    pub provider: String, // "ramp", "stitch", "ozow", "papss", "alchemypay", or "banxa"
    /// Payer name, required only by providers that create the checkout
    /// server-side (Stitch Express).
    #[serde(default)]
    pub payer_name: Option<String>,
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
/// redirect URL; providers that embed no secret (Ozow, Banxa) hold nothing.
///
/// `build_redirect_url` is synchronous and side-effect free. Adapters that must
/// create the checkout server-side (Stitch Express) instead implement
/// `create_hosted_checkout`, which is why the trait carries a defaulted async
/// method: a provider that embeds no secret needs no HTTP client at all.
#[async_trait]
pub trait FiatOnRampAdapter: Send + Sync {
    fn provider(&self) -> FiatOnRampProvider;
    fn build_redirect_url(&self, request: &OnRampSessionRequest, session_id: &str) -> String;

    /// Create the hosted checkout server-side and return the payer redirect URL.
    /// `Ok(None)` means this adapter does not create checkouts server-side, so the
    /// caller falls back to `build_redirect_url`.
    async fn create_hosted_checkout(
        &self,
        request: &OnRampSessionRequest,
        session_id: &str,
    ) -> ConxianResult<Option<String>> {
        let _ = (request, session_id);
        Ok(None)
    }
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

/// Stitch Express base host. Stitch Express is a **separate product** from the
/// Stitch Enterprise/Connect GraphQL API at `api.stitch.money`; it has its own
/// JSON token endpoint and payment-links endpoint. Test mode uses `test-`
/// prefixed credentials on this same host.
const STITCH_EXPRESS_BASE_URL: &str = "https://express.stitch.money";
/// Scope that authorises payment-request (payment-link) creation.
const STITCH_EXPRESS_SCOPE: &str = "client_paymentrequest";
/// Stitch Express minimum payment amount, in ZAR cents (R1.00).
const STITCH_MIN_AMOUNT_MINOR_UNITS: u64 = 100;

#[derive(Debug, Deserialize)]
struct StitchTokenEnvelope {
    success: bool,
    data: Option<StitchTokenData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StitchTokenData {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct StitchPaymentLinkEnvelope {
    success: bool,
    data: Option<StitchPaymentLinkData>,
}

#[derive(Debug, Deserialize)]
struct StitchPaymentLinkData {
    payment: Option<StitchPayment>,
}

#[derive(Debug, Deserialize)]
struct StitchPayment {
    link: String,
}

/// Convert a major-unit amount to Stitch Express integer ZAR cents. Returns
/// `None` when the amount is not finite, negative, or below the R1.00 minimum.
fn stitch_amount_minor_units(amount: f64) -> Option<u64> {
    if !amount.is_finite() || amount < 0.0 {
        return None;
    }
    let minor = (amount * 100.0).round();
    if !minor.is_finite() || minor < STITCH_MIN_AMOUNT_MINOR_UNITS as f64 {
        return None;
    }
    Some(minor as u64)
}

/// Stitch restricts `merchantReference` to `^[a-zA-Z0-9\s\-)]*$` (max 50 chars).
fn stitch_merchant_reference(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-' || *c == ')')
        .take(50)
        .collect()
}

/// Bank-agnostic South African on-ramp via **Stitch Express**: instant
/// bank-to-bank (EFT), card, and crypto-pay (settle crypto -> ZAR).
///
/// Unlike the redirect-only adapters, Stitch Express issues the hosted checkout
/// link **server-side**: the gateway exchanges its client credentials for a
/// short-lived token and creates a payment link, so no secret reaches the
/// payer-facing URL.
pub struct StitchAdapter {
    client_id: String,
    client_secret: String,
    base_url: String,
    client: reqwest::Client,
}

impl StitchAdapter {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
            base_url: STITCH_EXPRESS_BASE_URL.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Override the API host (sandbox or a test double).
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    fn token_url(&self) -> String {
        format!("{}/api/v1/token", self.base_url)
    }

    fn payment_links_url(&self) -> String {
        format!("{}/api/v1/payment-links", self.base_url)
    }

    /// Exchange the client credentials for a short-lived bearer token.
    async fn access_token(&self) -> ConxianResult<String> {
        let body = serde_json::json!({
            "clientId": self.client_id,
            "clientSecret": self.client_secret,
            "scope": STITCH_EXPRESS_SCOPE,
        });
        let response = self
            .client
            .post(self.token_url())
            .json(&body)
            .send()
            .await
            .map_err(|e| ConxianError::Api(format!("stitch token request failed: {e}")))?;
        let status = response.status();
        // The body is deliberately not echoed: a token response carries a
        // bearer token and must never reach logs or an error message.
        let text = response
            .text()
            .await
            .map_err(|e| ConxianError::Api(format!("stitch token read failed: {e}")))?;
        if !status.is_success() {
            return Err(ConxianError::Api(format!(
                "stitch token rejected (HTTP {})",
                status.as_u16()
            )));
        }
        let envelope: StitchTokenEnvelope = serde_json::from_str(&text).map_err(|_| {
            ConxianError::Api("stitch token response was not the expected shape".into())
        })?;
        if !envelope.success {
            return Err(ConxianError::Api(
                "stitch token response reported failure".into(),
            ));
        }
        envelope
            .data
            .map(|data| data.access_token)
            .filter(|token| !token.is_empty())
            .ok_or_else(|| {
                ConxianError::Api("stitch token response contained no access token".into())
            })
    }

    /// Create a Stitch Express payment link and return the payer redirect URL.
    async fn create_payment_link(
        &self,
        token: &str,
        request: &OnRampSessionRequest,
        session_id: &str,
    ) -> ConxianResult<String> {
        let payer_name = request
            .payer_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| ConxianError::Api("stitch on-ramp requires a payer name".to_string()))?;
        let amount = stitch_amount_minor_units(request.amount).ok_or_else(|| {
            ConxianError::Api("stitch on-ramp amount is below the R1.00 minimum".to_string())
        })?;
        let merchant_reference = stitch_merchant_reference(session_id);
        if merchant_reference.is_empty() {
            return Err(ConxianError::Api(
                "stitch on-ramp reference is empty after sanitising".to_string(),
            ));
        }

        let body = serde_json::json!({
            "amount": amount,
            "payerName": payer_name,
            "merchantReference": merchant_reference,
        });
        let response = self
            .client
            .post(self.payment_links_url())
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| ConxianError::Api(format!("stitch payment-link request failed: {e}")))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| ConxianError::Api(format!("stitch payment-link read failed: {e}")))?;
        if !status.is_success() {
            return Err(ConxianError::Api(format!(
                "stitch payment-link rejected (HTTP {})",
                status.as_u16()
            )));
        }
        let envelope: StitchPaymentLinkEnvelope = serde_json::from_str(&text).map_err(|_| {
            ConxianError::Api("stitch payment-link response was not the expected shape".into())
        })?;
        if !envelope.success {
            return Err(ConxianError::Api(
                "stitch payment-link response reported failure".into(),
            ));
        }
        envelope
            .data
            .and_then(|data| data.payment)
            .map(|payment| payment.link)
            .filter(|link| !link.is_empty())
            .ok_or_else(|| {
                ConxianError::Api("stitch payment-link response contained no link".into())
            })
    }
}

#[async_trait]
impl FiatOnRampAdapter for StitchAdapter {
    fn provider(&self) -> FiatOnRampProvider {
        FiatOnRampProvider::Stitch
    }

    /// Stitch Express checkouts are created server-side by
    /// `create_hosted_checkout`, which always supplies the real link. This
    /// value is a defensive fallback only: it is returned when the caller
    /// uses the generic redirect path instead of the hosted-checkout path,
    /// so it names the Stitch Express host rather than a payable link.
    fn build_redirect_url(&self, _request: &OnRampSessionRequest, _session_id: &str) -> String {
        self.base_url.clone()
    }

    async fn create_hosted_checkout(
        &self,
        request: &OnRampSessionRequest,
        session_id: &str,
    ) -> ConxianResult<Option<String>> {
        let token = self.access_token().await?;
        let link = self
            .create_payment_link(&token, request, session_id)
            .await?;
        Ok(Some(link))
    }
}

/// Verify a Stitch Express webhook signature.
///
/// Stitch Express delivers webhooks through Svix: the signed content is
/// `{svix-id}.{svix-timestamp}.{raw-body}`, HMAC-SHA256, Base64-encoded. The
/// signing secret is `whsec_`-prefixed and the remainder is Base64-decoded to
/// key bytes. `svix_signature` is a space-delimited list of `v1,<base64>` values.
/// The raw request body must be passed unmodified — re-serialising the JSON
/// breaks the HMAC.
pub fn verify_stitch_webhook(
    signing_secret: &str,
    svix_id: &str,
    svix_timestamp: &str,
    svix_signature: &str,
    raw_body: &str,
) -> ConxianResult<bool> {
    if signing_secret.is_empty() {
        return Err(ConxianError::Security(
            "stitch webhook signing secret is not configured".to_string(),
        ));
    }
    if svix_id.is_empty() || svix_timestamp.is_empty() || svix_signature.is_empty() {
        return Ok(false);
    }

    let key_material = signing_secret
        .strip_prefix("whsec_")
        .unwrap_or(signing_secret);
    let key = base64::engine::general_purpose::STANDARD
        .decode(key_material)
        .map_err(|_| {
            ConxianError::Security("stitch webhook signing secret is not valid base64".to_string())
        })?;

    let signed = format!("{svix_id}.{svix_timestamp}.{raw_body}");
    let mut mac = HmacSha256::new_from_slice(&key)
        .map_err(|e| ConxianError::Security(format!("HMAC error: {e}")))?;
    mac.update(signed.as_bytes());
    let expected = mac.finalize().into_bytes();

    for candidate in svix_signature.split(' ') {
        let Some(encoded) = candidate.strip_prefix("v1,") else {
            continue;
        };
        let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
            continue;
        };
        if decoded.len() == expected.len()
            && bool::from(decoded.as_slice().ct_eq(expected.as_slice()))
        {
            return Ok(true);
        }
    }
    Ok(false)
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
        // Providers that create the checkout server-side (Stitch Express)
        // return the payer URL here; redirect-only adapters fall back to the
        // stateless URL builder.
        let redirect_url = match adapter
            .create_hosted_checkout(&request, &session_id)
            .await?
        {
            Some(url) => url,
            None => adapter.build_redirect_url(&request, &session_id),
        };

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

        let provider =
            FiatOnRampProvider::from_str(&payload.provider).map_err(ConxianError::Security)?;
        if !self
            .adapters
            .iter()
            .any(|adapter| adapter.provider() == provider)
        {
            return Err(ConxianError::Security(
                "fiat webhook provider is not enabled".to_string(),
            ));
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
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_router() -> FiatRouter {
        FiatRouter::from_enabled(
            Some(RampAdapter::new("test-key".to_string())),
            Some(StitchAdapter::new(
                "stitch-id".to_string(),
                "stitch-secret".to_string(),
            )),
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
            payer_name: Some("Test Payer".to_string()),
        }
    }

    #[tokio::test]
    async fn test_create_ramp_session() {
        let res = test_router().create_session(request("ramp")).await.unwrap();
        assert_eq!(res.provider, "ramp");
        assert!(res.redirect_url.contains("bc1qtest"));
        assert!(res.redirect_url.contains("test-key"));
    }

    /// Stitch Express creates the checkout server-side: the adapter exchanges
    /// credentials for a token, then creates a payment link. Both calls are
    /// asserted against a WireMock double, including the minor-unit amount.
    #[tokio::test]
    async fn test_create_stitch_session_creates_server_side_payment_link() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/token"))
            .and(body_partial_json(
                serde_json::json!({ "scope": "client_paymentrequest" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "data": { "accessToken": "tok-123" }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/payment-links"))
            .and(body_partial_json(serde_json::json!({
                "amount": 10000,
                "payerName": "Test Payer"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "data": { "payment": { "id": "p1", "link": "https://express.stitch.money/AbC123" } }
            })))
            .mount(&server)
            .await;

        let adapter = StitchAdapter::new("stitch-id".to_string(), "stitch-secret".to_string())
            .with_base_url(server.uri());
        let router = FiatRouter::from_enabled(None, Some(adapter), None, None, None, None);
        let res = router.create_session(request("stitch")).await.unwrap();

        assert_eq!(res.provider, "stitch");
        assert_eq!(res.redirect_url, "https://express.stitch.money/AbC123");
    }

    #[tokio::test]
    async fn test_create_stitch_session_requires_payer_name() {
        let server = MockServer::start().await;
        let adapter = StitchAdapter::new("stitch-id".to_string(), "stitch-secret".to_string())
            .with_base_url(server.uri());
        let router = FiatRouter::from_enabled(None, Some(adapter), None, None, None, None);

        let mut req = request("stitch");
        req.payer_name = None;
        assert!(router.create_session(req).await.is_err());

        let mut req = request("stitch");
        req.payer_name = Some("   ".to_string());
        assert!(router.create_session(req).await.is_err());
    }

    #[tokio::test]
    async fn test_create_stitch_session_rejects_below_minimum() {
        let server = MockServer::start().await;
        let adapter = StitchAdapter::new("stitch-id".to_string(), "stitch-secret".to_string())
            .with_base_url(server.uri());
        let router = FiatRouter::from_enabled(None, Some(adapter), None, None, None, None);

        let mut req = request("stitch");
        req.amount = 0.5;
        assert!(router.create_session(req).await.is_err());
    }

    #[tokio::test]
    async fn test_create_stitch_session_fails_closed_on_token_rejection() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/token"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "success": false,
                "generalErrors": ["invalid_client"]
            })))
            .mount(&server)
            .await;

        let adapter = StitchAdapter::new("stitch-id".to_string(), "stitch-secret".to_string())
            .with_base_url(server.uri());
        let router = FiatRouter::from_enabled(None, Some(adapter), None, None, None, None);
        let err = router.create_session(request("stitch")).await.unwrap_err();
        // The credential body must never be echoed back in the error.
        assert!(!err.to_string().contains("stitch-secret"));
    }

    #[test]
    fn stitch_amounts_convert_to_minor_units() {
        assert_eq!(stitch_amount_minor_units(100.0), Some(10000));
        assert_eq!(stitch_amount_minor_units(1.0), Some(100));
        assert_eq!(stitch_amount_minor_units(0.5), None);
        assert_eq!(stitch_amount_minor_units(-1.0), None);
        assert_eq!(stitch_amount_minor_units(f64::NAN), None);
        assert_eq!(stitch_amount_minor_units(f64::INFINITY), None);
    }

    #[test]
    fn stitch_reference_is_sanitised_to_the_documented_charset() {
        let cleaned = stitch_merchant_reference("stitch-abc/../def?x=1");
        assert!(cleaned
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == ')'));
        assert!(cleaned.len() <= 50);
    }

    /// Build a Svix-style signature for the given secret/body.
    fn svix_signature(secret: &str, svix_id: &str, ts: &str, body: &str) -> String {
        let key = base64::engine::general_purpose::STANDARD
            .decode(secret.strip_prefix("whsec_").unwrap())
            .unwrap();
        let mut mac = HmacSha256::new_from_slice(&key).unwrap();
        mac.update(format!("{svix_id}.{ts}.{body}").as_bytes());
        format!(
            "v1,{}",
            base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
        )
    }

    #[test]
    fn stitch_webhook_verification_accepts_a_valid_svix_signature() {
        let secret = format!(
            "whsec_{}",
            base64::engine::general_purpose::STANDARD.encode(b"stitch-signing-key-material")
        );
        let body = r#"{"amount":5000,"id":"pay-1","status":"PAID","type":"LINK"}"#;
        let sig = svix_signature(&secret, "msg_1", "1700000000", body);
        assert!(verify_stitch_webhook(&secret, "msg_1", "1700000000", &sig, body).unwrap());
    }

    #[test]
    fn stitch_webhook_verification_rejects_tampering() {
        let secret = format!(
            "whsec_{}",
            base64::engine::general_purpose::STANDARD.encode(b"stitch-signing-key-material")
        );
        let body = r#"{"amount":5000,"status":"PAID"}"#;
        let sig = svix_signature(&secret, "msg_1", "1700000000", body);
        // Body changed after signing -> must not verify.
        assert!(!verify_stitch_webhook(
            &secret,
            "msg_1",
            "1700000000",
            &sig,
            r#"{"amount":999999,"status":"PAID"}"#
        )
        .unwrap());
        // Different message id -> must not verify.
        assert!(!verify_stitch_webhook(&secret, "msg_2", "1700000000", &sig, body).unwrap());
    }

    #[test]
    fn stitch_webhook_verification_fails_closed_without_a_secret() {
        assert!(verify_stitch_webhook("", "msg_1", "1700000000", "v1,AAAA", "{}").is_err());
    }

    #[test]
    fn stitch_webhook_verification_rejects_a_malformed_secret() {
        assert!(verify_stitch_webhook("whsec_not-base64!!", "m", "1", "v1,AAAA", "{}").is_err());
    }

    #[tokio::test]
    async fn test_create_ozow_session() {
        let res = test_router().create_session(request("ozow")).await.unwrap();
        assert_eq!(res.provider, "ozow");
        assert!(res.redirect_url.contains("ozow.com"));
    }

    #[tokio::test]
    async fn test_create_papss_session() {
        let res = test_router()
            .create_session(request("papss"))
            .await
            .unwrap();
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

    #[test]
    fn disabled_provider_rejects_even_a_correctly_signed_webhook() {
        let (payload, secret) = signed_webhook("ramp", "webhook-secret");
        let disabled = FiatRouter::from_enabled(None, None, None, None, None, None);
        assert!(disabled.verify_webhook(&payload, &secret).is_err());
        let another = FiatRouter::from_enabled(
            None,
            Some(StitchAdapter::new(
                "stitch-id".to_string(),
                "stitch-secret".to_string(),
            )),
            None,
            None,
            None,
            None,
        );
        assert!(another.verify_webhook(&payload, &secret).is_err());
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
