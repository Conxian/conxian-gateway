//! Conxian device lifecycle contract.
//!
//! One typed, signed desired-state manifest is shared by both tracks of the
//! hybrid device-lifecycle design:
//!
//! * **Track A (local install)** applies the manifest's typed actions directly
//!   on the operator's machine.
//! * **Track B (fleet agent)** ships the same manifest over the network; the
//!   local machine *pulls* it, verifies the signature, plans, and applies.
//!
//! The server never pushes commands and never runs anything: it holds desired
//! state and signs manifests. There is deliberately no `runShell`/`exec` action
//! kind — an unknown kind fails to deserialize, which is the fail-closed
//! behaviour required by the design.

pub mod apply;
pub mod error;
pub mod manifest;
pub mod plan;
pub mod signing;

pub use apply::{apply, ActionExecutor, ApplyReport, Consent, StepOutcome};
pub use error::DeviceError;
pub use manifest::{ActionKind, DesiredStateManifest, SignedManifest, SCHEMA_VERSION};
pub use plan::{manifest_digest, plan, ObservedState, Plan, PlanStep, StepStatus};
pub use signing::{sign_manifest, verify_manifest};
