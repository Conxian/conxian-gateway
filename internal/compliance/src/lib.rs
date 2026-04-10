#[cfg(all(feature = "mock-integrations", not(any(debug_assertions, test))))]
compile_error!("feature `mock-integrations` must not be enabled in release builds");

pub mod identity;
pub mod zkc;

pub use identity::IdentityManager;
pub use zkc::ZkcVerifier;
