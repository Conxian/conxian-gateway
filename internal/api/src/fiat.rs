use conxian_core::{ConxianError, ConxianResult};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::info;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OnRampSessionRequest {
    pub wallet_address: String,
    pub amount: f64,
    pub currency: String,
    pub provider: String, // "ramp", "investec", "alchemypay", or "banxa"
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

pub struct FiatRouter {
    ramp_api_key: String,
    #[allow(dead_code)]
    investec_client_id: String,
    #[allow(dead_code)]
    investec_secret: String,
    #[allow(dead_code)]
    alchemy_pay_app_id: String,
    #[allow(dead_code)]
    alchemy_pay_secret: String,
    #[allow(dead_code)]
    banxa_api_key: String,
    #[allow(dead_code)]
    banxa_secret: String,
}

impl FiatRouter {
    pub fn new(
        ramp_api_key: String,
        investec_client_id: String,
        investec_secret: String,
        alchemy_pay_app_id: String,
        alchemy_pay_secret: String,
        banxa_api_key: String,
        banxa_secret: String,
    ) -> Self {
        Self {
            ramp_api_key,
            investec_client_id,
            investec_secret,
            alchemy_pay_app_id,
            alchemy_pay_secret,
            banxa_api_key,
            banxa_secret,
        }
    }

    pub async fn create_session(
        &self,
        request: OnRampSessionRequest,
    ) -> ConxianResult<OnRampSessionResponse> {
        info!(
            "Creating on-ramp session for {} via {}",
            request.wallet_address, request.provider
        );

        match request.provider.as_str() {
            "ramp" => self.create_ramp_session(request).await,
            "investec" => self.create_investec_session(request).await,
            "alchemypay" => self.create_alchemypay_session(request).await,
            "banxa" => self.create_banxa_session(request).await,
            _ => Err(ConxianError::Api(format!(
                "Unsupported provider: {}",
                request.provider
            ))),
        }
    }

    async fn create_ramp_session(
        &self,
        request: OnRampSessionRequest,
    ) -> ConxianResult<OnRampSessionResponse> {
        let session_id = format!("ramp-{}", uuid::Uuid::new_v4());
        let redirect_url = format!(
            "https://buy.ramp.network/?userAddress={}&swapAmount={}&swapAsset={}&apiKey={}",
            request.wallet_address, request.amount, request.currency, self.ramp_api_key
        );

        Ok(OnRampSessionResponse {
            session_id,
            redirect_url,
            provider: "ramp".to_string(),
        })
    }

    async fn create_investec_session(
        &self,
        request: OnRampSessionRequest,
    ) -> ConxianResult<OnRampSessionResponse> {
        let session_id = format!("investec-{}", uuid::Uuid::new_v4());
        let redirect_url = format!(
            "https://investec.com/banking/pay?ref={}&amount={}",
            session_id, request.amount
        );

        Ok(OnRampSessionResponse {
            session_id,
            redirect_url,
            provider: "investec".to_string(),
        })
    }

    async fn create_alchemypay_session(
        &self,
        request: OnRampSessionRequest,
    ) -> ConxianResult<OnRampSessionResponse> {
        // Industry Enhancement: Alchemy Pay Integration (CON-41)
        let session_id = format!("alchemypay-{}", uuid::Uuid::new_v4());
        let redirect_url = format!(
            "https://ramp.alchemypay.org/?address={}&cryptoAmount={}&crypto={}&appId={}",
            request.wallet_address, request.amount, request.currency, self.alchemy_pay_app_id
        );

        Ok(OnRampSessionResponse {
            session_id,
            redirect_url,
            provider: "alchemypay".to_string(),
        })
    }

    async fn create_banxa_session(
        &self,
        request: OnRampSessionRequest,
    ) -> ConxianResult<OnRampSessionResponse> {
        // Industry Enhancement: Banxa Integration (CON-41)
        let session_id = format!("banxa-{}", uuid::Uuid::new_v4());
        let redirect_url = format!(
            "https://conxian-labs.banxa.com/?walletAddress={}&coinAmount={}&coinType={}",
            request.wallet_address, request.amount, request.currency
        );

        Ok(OnRampSessionResponse {
            session_id,
            redirect_url,
            provider: "banxa".to_string(),
        })
    }

    pub fn verify_webhook(&self, payload: &WebhookPayload, secret: &str) -> ConxianResult<bool> {
        match payload.provider.as_str() {
            "ramp" => self.verify_ramp_webhook(payload, secret),
            "investec" => self.verify_investec_webhook(payload, secret),
            "alchemypay" => self.verify_alchemypay_webhook(payload, secret),
            "banxa" => self.verify_banxa_webhook(payload, secret),
            _ => Err(ConxianError::Security(
                "Unknown webhook provider".to_string(),
            )),
        }
    }

    fn verify_ramp_webhook(&self, payload: &WebhookPayload, secret: &str) -> ConxianResult<bool> {
        if payload.signature.is_empty() {
            return Ok(false);
        }

        info!(
            "Verifying Ramp webhook HMAC signature for reference: {}",
            payload.reference_id
        );

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|e| ConxianError::Security(format!("HMAC error: {}", e)))?;
        mac.update(payload.raw_payload.as_bytes());

        let sig_bytes = hex::decode(&payload.signature)
            .map_err(|e| ConxianError::Security(format!("Invalid signature hex: {}", e)))?;

        Ok(mac.verify_slice(&sig_bytes).is_ok())
    }

    fn verify_investec_webhook(
        &self,
        payload: &WebhookPayload,
        secret: &str,
    ) -> ConxianResult<bool> {
        if secret.is_empty() {
            return Err(ConxianError::Security(
                "Investec webhook secret is not configured".to_string(),
            ));
        }

        if payload.signature.is_empty() {
            return Ok(false);
        }

        info!(
            "Verifying Investec webhook HMAC signature for reference: {}",
            payload.reference_id
        );

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|e| ConxianError::Security(format!("HMAC error: {}", e)))?;
        mac.update(payload.raw_payload.as_bytes());

        let sig_bytes = hex::decode(&payload.signature)
            .map_err(|e| ConxianError::Security(format!("Invalid signature hex: {}", e)))?;

        Ok(mac.verify_slice(&sig_bytes).is_ok())
    }

    fn verify_alchemypay_webhook(
        &self,
        payload: &WebhookPayload,
        secret: &str,
    ) -> ConxianResult<bool> {
        // Industry Enhancement: Alchemy Pay Signature Verification (CON-41)
        if payload.signature.is_empty() {
            return Ok(false);
        }

        info!(
            "Verifying Alchemy Pay webhook signature for reference: {}",
            payload.reference_id
        );

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|e| ConxianError::Security(format!("HMAC error: {}", e)))?;
        mac.update(payload.raw_payload.as_bytes());

        let sig_bytes = hex::decode(&payload.signature)
            .map_err(|e| ConxianError::Security(format!("Invalid signature hex: {}", e)))?;

        Ok(mac.verify_slice(&sig_bytes).is_ok())
    }

    fn verify_banxa_webhook(&self, payload: &WebhookPayload, secret: &str) -> ConxianResult<bool> {
        // Industry Enhancement: Banxa Signature Verification (CON-41)
        if payload.signature.is_empty() {
            return Ok(false);
        }

        info!(
            "Verifying Banxa webhook signature for reference: {}",
            payload.reference_id
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

    #[tokio::test]
    async fn test_create_ramp_session() {
        let router = FiatRouter::new(
            "test-key".to_string(),
            "client-id".to_string(),
            "secret".to_string(),
            "ap-app-id".to_string(),
            "ap-secret".to_string(),
            "banxa-key".to_string(),
            "banxa-secret".to_string(),
        );

        let req = OnRampSessionRequest {
            wallet_address: "bc1qtest".to_string(),
            amount: 100.0,
            currency: "BTC".to_string(),
            provider: "ramp".to_string(),
        };

        let res = router.create_session(req).await.unwrap();
        assert_eq!(res.provider, "ramp");
        assert!(res.redirect_url.contains("bc1qtest"));
        assert!(res.redirect_url.contains("test-key"));
    }

    #[tokio::test]
    async fn test_create_alchemypay_session() {
        let router = FiatRouter::new(
            "test-key".to_string(),
            "client-id".to_string(),
            "secret".to_string(),
            "ap-app-id".to_string(),
            "ap-secret".to_string(),
            "banxa-key".to_string(),
            "banxa-secret".to_string(),
        );

        let req = OnRampSessionRequest {
            wallet_address: "bc1qtest".to_string(),
            amount: 50.0,
            currency: "ETH".to_string(),
            provider: "alchemypay".to_string(),
        };

        let res = router.create_session(req).await.unwrap();
        assert_eq!(res.provider, "alchemypay");
        assert!(res.redirect_url.contains("bc1qtest"));
        assert!(res.redirect_url.contains("ap-app-id"));
    }

    #[tokio::test]
    async fn test_create_banxa_session() {
        let router = FiatRouter::new(
            "test-key".to_string(),
            "client-id".to_string(),
            "secret".to_string(),
            "ap-app-id".to_string(),
            "ap-secret".to_string(),
            "banxa-key".to_string(),
            "banxa-secret".to_string(),
        );

        let req = OnRampSessionRequest {
            wallet_address: "bc1qtest".to_string(),
            amount: 200.0,
            currency: "USDT".to_string(),
            provider: "banxa".to_string(),
        };

        let res = router.create_session(req).await.unwrap();
        assert_eq!(res.provider, "banxa");
        assert!(res.redirect_url.contains("bc1qtest"));
        assert!(res.redirect_url.contains("USDT"));
    }

    #[tokio::test]
    async fn test_verify_ramp_webhook() {
        let router = FiatRouter::new(
            "test-key".to_string(),
            "client-id".to_string(),
            "secret".to_string(),
            "ap-app-id".to_string(),
            "ap-secret".to_string(),
            "banxa-key".to_string(),
            "banxa-secret".to_string(),
        );

        let secret = "webhook-secret";
        let raw_payload = r#"{"reference":"ref123","status":"SUCCESS"}"#;

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(raw_payload.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        let payload = WebhookPayload {
            provider: "ramp".to_string(),
            event_type: "ORDER_CREATED".to_string(),
            reference_id: "ref123".to_string(),
            amount: 100.0,
            status: "SUCCESS".to_string(),
            signature,
            raw_payload: raw_payload.to_string(),
        };

        let valid = router.verify_webhook(&payload, secret).unwrap();
        assert!(valid);
    }

    #[tokio::test]
    async fn test_verify_banxa_webhook() {
        let router = FiatRouter::new(
            "test-key".to_string(),
            "client-id".to_string(),
            "secret".to_string(),
            "ap-app-id".to_string(),
            "ap-secret".to_string(),
            "banxa-key".to_string(),
            "banxa-secret".to_string(),
        );

        let secret = "banxa-secret";
        let raw_payload = r#"{"orderId":"banxa-123","status":"completed"}"#;

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(raw_payload.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        let payload = WebhookPayload {
            provider: "banxa".to_string(),
            event_type: "ORDER_COMPLETED".to_string(),
            reference_id: "banxa-123".to_string(),
            amount: 200.0,
            status: "completed".to_string(),
            signature,
            raw_payload: raw_payload.to_string(),
        };

        let valid = router.verify_webhook(&payload, secret).unwrap();
        assert!(valid);
    }

    #[tokio::test]
    async fn test_verify_investec_webhook() {
        let router = FiatRouter::new(
            "test-key".to_string(),
            "client-id".to_string(),
            "secret".to_string(),
            "ap-app-id".to_string(),
            "ap-secret".to_string(),
            "banxa-key".to_string(),
            "banxa-secret".to_string(),
        );

        let secret = "investec-secret";
        let raw_payload = r#"{"reference":"investec-123","status":"approved"}"#;

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(raw_payload.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        let payload = WebhookPayload {
            provider: "investec".to_string(),
            event_type: "PAYMENT_APPROVED".to_string(),
            reference_id: "investec-123".to_string(),
            amount: 300.0,
            status: "approved".to_string(),
            signature,
            raw_payload: raw_payload.to_string(),
        };

        let valid = router.verify_webhook(&payload, secret).unwrap();
        assert!(valid);
    }

    #[tokio::test]
    async fn test_verify_investec_webhook_fails_closed_when_secret_missing() {
        let router = FiatRouter::new(
            "test-key".to_string(),
            "client-id".to_string(),
            "secret".to_string(),
            "ap-app-id".to_string(),
            "ap-secret".to_string(),
            "banxa-key".to_string(),
            "banxa-secret".to_string(),
        );

        let payload = WebhookPayload {
            provider: "investec".to_string(),
            event_type: "PAYMENT_APPROVED".to_string(),
            reference_id: "investec-123".to_string(),
            amount: 300.0,
            status: "approved".to_string(),
            signature: "aabbcc".to_string(),
            raw_payload: r#"{"reference":"investec-123","status":"approved"}"#.to_string(),
        };

        let result = router.verify_webhook(&payload, "");
        assert!(result.is_err());
    }
}
