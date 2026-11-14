//! Typed, versioned desired-state manifest.
//!
//! The manifest is the single shared contract between the local-install track
//! and the fleet-agent track. It is deliberately *typed*: only known action
//! kinds can be represented, and unknown kinds fail to deserialize. There is no
//! action kind that runs arbitrary code.

use serde::{Deserialize, Serialize};

use crate::error::{DeviceError, DeviceResult};

/// The only schema version this crate understands.
pub const SCHEMA_VERSION: u32 = 1;

/// A typed, idempotent change the agent may apply locally.
///
/// Every variant maps to a documented inverse (see `apply`) or a safe no-op, so
/// rollback is always possible. Values that are secret (API keys, DB URLs) are
/// never carried in the manifest — only the *names* of keys to ensure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ActionKind {
    /// Ensure a set of environment keys exist and are non-empty.
    ///
    /// The manifest carries key *names* only; the values live in the local
    /// store. A missing key is a plan-time failure, not a secret in transit.
    EnsureEnv { keys: Vec<String> },

    /// Apply the schema migrations for a named target, idempotently.
    MigrateDatabase { target: String },

    /// Pull an image pinned by digest. A tag (mutable) is refused.
    PullImage { image: String },

    /// Restart a named service. Idempotent: restarting an already-running
    /// service converges to the same desired state.
    RestartService { service: String },
}

impl ActionKind {
    /// A short, stable label used in plans and audit output.
    pub fn label(&self) -> &'static str {
        match self {
            ActionKind::EnsureEnv { .. } => "ensureEnv",
            ActionKind::MigrateDatabase { .. } => "migrateDatabase",
            ActionKind::PullImage { .. } => "pullImage",
            ActionKind::RestartService { .. } => "restartService",
        }
    }

    /// The deterministic identity of the action, used to detect whether an
    /// action is already satisfied by observed state. Two actions with the same
    /// key produce the same effect, so a matching observed value means no-op.
    pub fn state_key(&self) -> String {
        match self {
            ActionKind::EnsureEnv { keys } => {
                let mut k = keys.clone();
                k.sort();
                format!("env:{}", k.join(","))
            }
            ActionKind::MigrateDatabase { target } => format!("migrate:{target}"),
            ActionKind::PullImage { image } => format!("image:{image}"),
            ActionKind::RestartService { service } => format!("service:{service}"),
        }
    }
}

/// A signed, versioned desired-state manifest for one device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesiredStateManifest {
    pub schema_version: u32,
    pub device: String,
    pub issued_at: String,
    /// Optional expiry; when present the agent refuses a manifest past it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub actions: Vec<ActionKind>,
}

impl DesiredStateManifest {
    pub fn new(device: impl Into<String>, issued_at: impl Into<String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            device: device.into(),
            issued_at: issued_at.into(),
            expires_at: None,
            actions: Vec::new(),
        }
    }

    /// Reject an unsupported schema version. Called by every consumer before
    /// the manifest is used, so a future format cannot be silently misapplied.
    pub fn check_schema(&self) -> DeviceResult<()> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(DeviceError::UnsupportedSchema {
                found: self.schema_version,
                expected: SCHEMA_VERSION,
            });
        }
        Ok(())
    }

    /// A manifest with no actions is valid and means "converged".
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

/// A manifest together with its detached signature, exactly as it travels
/// between the control plane and the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignedManifest {
    pub manifest: DesiredStateManifest,
    /// Hex-encoded x-only public key of the signer, so the verifier can select
    /// the trusted key rather than trust an in-band key.
    pub signer: String,
    /// Hex-encoded BIP-340 Schnorr signature over the canonical manifest bytes.
    pub signature: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_action_kind_is_refused_at_parse_time() {
        // The design's fail-closed rule: a manifest carrying an action kind the
        // agent does not know must not parse into something runnable.
        let raw = r#"{
            "schemaVersion": 1,
            "device": "d1",
            "issuedAt": "2026-11-14T00:00:00Z",
            "actions": [ { "kind": "runShell", "cmd": "rm -rf /" } ]
        }"#;
        let err = serde_json::from_str::<DesiredStateManifest>(raw).unwrap_err();
        assert!(err.to_string().contains("runShell") || err.to_string().contains("unknown"));
    }

    #[test]
    fn unknown_field_is_refused() {
        let raw = r#"{
            "schemaVersion": 1,
            "device": "d1",
            "issuedAt": "2026-11-14T00:00:00Z",
            "actions": [],
            "surprise": true
        }"#;
        assert!(serde_json::from_str::<DesiredStateManifest>(raw).is_err());
    }

    #[test]
    fn known_actions_round_trip() {
        let mut m = DesiredStateManifest::new("d1", "2026-11-14T00:00:00Z");
        m.actions.push(ActionKind::EnsureEnv {
            keys: vec!["B".into(), "A".into()],
        });
        m.actions.push(ActionKind::MigrateDatabase {
            target: "control-plane".into(),
        });
        m.actions.push(ActionKind::PullImage {
            image: "ghcr.io/conxian/gateway@sha256:abc".into(),
        });
        m.actions.push(ActionKind::RestartService {
            service: "gateway".into(),
        });
        let json = serde_json::to_string(&m).unwrap();
        let back: DesiredStateManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
        m.check_schema().unwrap();
    }

    #[test]
    fn unsupported_schema_is_rejected() {
        let mut m = DesiredStateManifest::new("d1", "2026-11-14T00:00:00Z");
        m.schema_version = 99;
        assert!(matches!(
            m.check_schema(),
            Err(DeviceError::UnsupportedSchema { found: 99, .. })
        ));
    }

    #[test]
    fn state_key_is_order_independent_for_env() {
        let a = ActionKind::EnsureEnv {
            keys: vec!["A".into(), "B".into()],
        };
        let b = ActionKind::EnsureEnv {
            keys: vec!["B".into(), "A".into()],
        };
        assert_eq!(a.state_key(), b.state_key());
    }
}
