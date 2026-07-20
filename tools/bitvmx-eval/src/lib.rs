#![cfg_attr(not(feature = "bitvmx-eval"), allow(unused))]

#[cfg(feature = "bitvmx-eval")]
pub mod cli;
#[cfg(feature = "bitvmx-eval")]
pub mod error;
#[cfg(feature = "bitvmx-eval")]
pub mod model;
#[cfg(feature = "bitvmx-eval")]
pub mod runner;

#[cfg(feature = "bitvmx-eval")]
pub use error::EvalError;
#[cfg(feature = "bitvmx-eval")]
pub use model::{
    ArtifactSpec, ExecutableSpec, ExecutionSpec, FixtureSpec, LimitsSpec, Manifest, Report,
    SandboxSpec, WARNING,
};
#[cfg(feature = "bitvmx-eval")]
pub use runner::{run_manifest, sha256_file};
