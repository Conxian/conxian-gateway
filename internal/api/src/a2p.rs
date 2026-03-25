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
    #[allow(dead_code)] infobip_api_key: String,
    infobip_base_url: String,
    hmac_secret: String,
}

impl A2pRouter {
    pub fn new(#[allow(dead_code)] infobip_api_key: String, infobip_base_url: String, hmac_secret: String) -> Self {
        Self {
            infobip_api_key,
            infobip_base_url,
            hmac_secret,
        }
    }

    /// Sends an OTP via Infobip and returns a stateless session.
    pub async fn send_otp(&self, request: OtpRequest) -> ConxianResult<(OtpResponse, String, u64)> {
        info!(
            "Sending OTP to {} via {} using Infobip",
            request.phone_number, request.channel
        );

        // In a real implementation, we would call Infobip API here.
        // Simulation: generate a 6-digit OTP code (normally Infobip handles this, but for stateless we might generate it)
        let otp_code = "123456"; // Mock OTP
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Generate HMAC for stateless verification (CON-40)
        let hmac_value = self.generate_hmac(&request.phone_number, otp_code, timestamp)?;

        let session_id = uuid::Uuid::new_v4().to_string();

        // Mock Infobip API call
        info!("Infobip API call to {}: sending {} to {}", self.infobip_base_url, otp_code, request.phone_number);

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

        let expected_hmac = self.generate_hmac(&request.phone_number, &request.otp_code, request.timestamp)?;

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

#[cfg(test)]
mod tests {
    use super::*;

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

        let verify_req = OtpVerificationRequest {
            session_id: res.session_id,
            otp_code: "123456".to_string(),
            phone_number: phone,
            hmac,
            timestamp: ts,
        };

        let valid = router.verify_otp(verify_req).unwrap();
        assert!(valid);
    }
}
