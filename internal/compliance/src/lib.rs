#[cfg(all(feature = "mock-integrations", not(any(debug_assertions, test))))]
compile_error!("feature mock-integrations must not be enabled in release builds");

pub mod identity;
pub mod zkc;

pub use identity::IdentityManager;
pub use zkc::ZkcVerifier;

pub trait SovereignCommit: Send + Sync {
    fn commit_settlement(
        &self,
        envelope: &conxian_core::SettlementEnvelope,
    ) -> conxian_core::ConxianResult<()>;
    fn commit_job_card(
        &self,
        job_card: &conxian_core::ConxianJobCard,
    ) -> conxian_core::ConxianResult<()>;
}
