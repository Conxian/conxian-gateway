use conxian_core::{ConxianError, ConxianResult};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OtpRequest {
    pub phone_number: String,
    pub channel: String, // "sms" or "whatsapp"
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OtpResponse {
    pub session_id: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OtpVerificationRequest {
    pub session_id: String,
    pub otp_code: String,
    pub phone_number: String,
    pub hmac: String,
    pub timestamp: u64,
}

pub struct A2pRouter {
    #[allow(dead_code)]
    infobip_api_key: String,
    #[allow(dead_code)]
    infobip_base_url: String,
    hmac_secret: String,
}

impl A2pRouter {
    pub fn new(
        #[allow(dead_code)] infobip_api_key: String,
        infobip_base_url: String,
        hmac_secret: String,
    ) -> Self {
        Self {
            infobip_api_key,
            infobip_base_url,
            hmac_secret,
        }
    }

    /// Sends an OTP and returns a stateless session.
    ///
    /// This is intentionally disabled in production builds until the Infobip integration is
    /// implemented. To run stateless OTP flows in development or unit tests, enable the
    /// `mock-integrations` feature.
    #[cfg(not(any(test, feature = "mock-integrations")))]
    pub async fn send_otp(
        &self,
        request: OtpRequest,
    ) -> ConxianResult<(OtpResponse, String, u64)> {
        let phone_tail = phone_tail(&request.phone_number);
        info!(
            phone_tail = %phone_tail,
            channel = %request.channel,
            "A2P OTP sending is disabled in this build"
        );

        Err(ConxianError::Security(
            "A2P OTP sending is disabled (requires Infobip integration)".to_string(),
        ))
    }

    #[cfg(any(test, feature = "mock-integrations"))]
    pub async fn send_otp(
        &self,
        request: OtpRequest,
    ) -> ConxianResult<(OtpResponse, String, u64)> {
        let phone_tail = phone_tail(&request.phone_number);
        info!(
            phone_tail = %phone_tail,
            channel = %request.channel,
            "Sending OTP via mock A2P provider"
        );

        let otp_code = generate_otp_code();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let hmac_value = self.generate_hmac(&request.phone_number, &otp_code, timestamp)?;
        let session_id = uuid::Uuid::new_v4().to_string();

        info!(base_url = %self.infobip_base_url, "Mock A2P delivery completed");

        Ok((
            OtpResponse {
                session_id,
                status: "sent".to_string(),
            },
            hmac_value,
            timestamp,
        ))
    }

    pub fn verify_otp(&self, request: OtpVerificationRequest) -> ConxianResult<bool> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Check expiration (e.g., 5 minutes)
        if now.saturating_sub(request.timestamp) > 300 {
            return Err(ConxianError::Security("OTP has expired".to_string()));
        }

        let expected_hmac =
            self.generate_hmac(&request.phone_number, &request.otp_code, request.timestamp)?;

        if request.hmac != expected_hmac {
            return Ok(false);
        }

        Ok(true)
    }

    fn generate_hmac(&self, phone: &str, code: &str, timestamp: u64) -> ConxianResult<String> {
        let mut mac = HmacSha256::new_from_slice(self.hmac_secret.as_bytes())
            .map_err(|e| ConxianError::Security(format!("HMAC error: {}", e)))?;

        let data = format!("{}:{}:{}", phone, code, timestamp);
        mac.update(data.as_bytes());

        Ok(hex::encode(mac.finalize().into_bytes()))
    }
}

/// Returns up to the last 4 characters of a phone number for logging.
///
/// This avoids panics on short or non-ASCII inputs.
fn phone_tail(phone_number: &str) -> &str {
    let start = phone_number
        .char_indices()
        .rev()
        .nth(3)
        .map(|(idx, _)| idx)
        .unwrap_or(0);

    &phone_number[start..]
}

#[cfg(test)]
const TEST_OTP_CODE: &str = "000001";

#[cfg(test)]
fn generate_otp_code() -> String {
    TEST_OTP_CODE.to_string()
}

#[cfg(all(feature = "mock-integrations", not(test)))]
fn generate_otp_code() -> String {
    use rand::{rngs::OsRng, Rng};

    format!("{:06}", OsRng.gen_range(0..1_000_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_tail_returns_tail() {
        assert_eq!(phone_tail("+1234567890"), "7890");
        assert_eq!(phone_tail("123"), "123");
    }

    #[tokio::test]
    async fn test_stateless_otp_flow() {
        let router = A2pRouter::new(
            "infobip-key".to_string(),
            "https://api.infobip.com".to_string(),
            "hmac-secret".to_string(),
        );

        let phone = "+1234567890".to_string();
        let req = OtpRequest {
            phone_number: phone.clone(),
            channel: "sms".to_string(),
        };

        let (res, hmac, ts) = router.send_otp(req).await.unwrap();
        assert_eq!(res.status, "sent");
        assert_eq!(hmac.len(), 64);

        let otp_code = TEST_OTP_CODE.to_string();

        let verify_req = OtpVerificationRequest {
            session_id: res.session_id,
            otp_code,
            phone_number: phone.clone(),
            hmac,
            timestamp: ts,
        };

        let valid = router.verify_otp(verify_req).unwrap();
        assert!(valid);
    }
}
