//! Error domain for the device lifecycle contract.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("manifest schema version {found} is not supported (expected {expected})")]
    UnsupportedSchema { found: u32, expected: u32 },

    #[error("manifest is malformed: {0}")]
    Malformed(String),

    #[error("manifest signature is missing")]
    MissingSignature,

    #[error("manifest signature could not be parsed")]
    BadSignature,

    #[error("manifest signature did not verify against the trusted key")]
    SignatureInvalid,

    #[error("signing key material could not be read")]
    BadSigningKey,

    #[error("manifest signature has expired (expires_at {expires_at}, now {now})")]
    Expired { expires_at: String, now: String },

    #[error("no signature is present, so nothing can be verified")]
    NothingToVerify,
}

pub type DeviceResult<T> = Result<T, DeviceError>;
