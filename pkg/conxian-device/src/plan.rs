//! Plan engine: diff the desired manifest against observed local state.
//!
//! Planning is pure and side-effect free. It produces, per action, a step that
//! is either `Pending` (needs applying) or `Satisfied` (already converged, so
//! applying it is a no-op). This is what makes apply idempotent: re-running a
//! plan over unchanged state yields only `Satisfied` steps.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::DeviceResult;
use crate::manifest::{ActionKind, DesiredStateManifest};

/// What the agent can observe about the local machine.
///
/// Each field is a set of state keys already satisfied locally. The apply
/// engine is responsible for reporting the post-apply state; this struct is
/// only the *observed* input to planning.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedState {
    /// State keys (`ActionKind::state_key`) already satisfied.
    #[serde(default)]
    pub satisfied: Vec<String>,
    /// Environment key names that are present and non-empty locally.
    #[serde(default)]
    pub env_keys_present: Vec<String>,
}

impl ObservedState {
    pub fn is_satisfied(&self, key: &str) -> bool {
        self.satisfied.iter().any(|k| k == key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepStatus {
    Pending,
    Satisfied,
    /// The action cannot be planned because a precondition is unmet locally
    /// (for example an `ensureEnv` key that has no value in the local store).
    Blocked {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub index: usize,
    pub action: ActionKind,
    pub status: StepStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub device: String,
    pub manifest_digest: String,
    pub steps: Vec<PlanStep>,
}

impl Plan {
    /// True when every step is already satisfied — nothing to apply.
    pub fn is_noop(&self) -> bool {
        self.steps
            .iter()
            .all(|s| matches!(s.status, StepStatus::Satisfied))
    }

    /// True when at least one step is blocked, so the plan must not be applied.
    pub fn has_blockers(&self) -> bool {
        self.steps
            .iter()
            .any(|s| matches!(s.status, StepStatus::Blocked { .. }))
    }
}

/// A stable digest of the manifest contents, recorded with the plan and the
/// audit event so a report can be tied to the exact manifest that produced it.
pub fn manifest_digest(manifest: &DesiredStateManifest) -> DeviceResult<String> {
    let bytes = serde_json::to_vec(manifest)
        .map_err(|e| crate::error::DeviceError::Malformed(e.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

/// Build a plan for a verified manifest against observed local state.
pub fn plan(manifest: &DesiredStateManifest, observed: &ObservedState) -> DeviceResult<Plan> {
    manifest.check_schema()?;
    let mut steps = Vec::with_capacity(manifest.actions.len());
    for (index, action) in manifest.actions.iter().enumerate() {
        let status = match action {
            ActionKind::EnsureEnv { keys } => {
                let missing: Vec<&String> = keys
                    .iter()
                    .filter(|k| !observed.env_keys_present.iter().any(|p| p == *k))
                    .collect();
                if missing.is_empty() {
                    StepStatus::Satisfied
                } else {
                    StepStatus::Blocked {
                        reason: format!(
                            "missing local value for key(s): {}",
                            missing
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    }
                }
            }
            other => {
                if observed.is_satisfied(&other.state_key()) {
                    StepStatus::Satisfied
                } else {
                    StepStatus::Pending
                }
            }
        };
        steps.push(PlanStep {
            index,
            action: action.clone(),
            status,
        });
    }
    Ok(Plan {
        device: manifest.device.clone(),
        manifest_digest: manifest_digest(manifest)?,
        steps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_with(actions: Vec<ActionKind>) -> DesiredStateManifest {
        let mut m = DesiredStateManifest::new("d1", "2026-11-14T00:00:00Z");
        m.actions = actions;
        m
    }

    #[test]
    fn empty_manifest_is_a_noop() {
        let m = manifest_with(vec![]);
        let p = plan(&m, &ObservedState::default()).unwrap();
        assert!(p.is_noop());
        assert!(!p.has_blockers());
    }

    #[test]
    fn satisfied_action_plans_as_noop() {
        let m = manifest_with(vec![ActionKind::RestartService {
            service: "gateway".into(),
        }]);
        let observed = ObservedState {
            satisfied: vec!["service:gateway".into()],
            ..Default::default()
        };
        let p = plan(&m, &observed).unwrap();
        assert!(p.is_noop());
    }

    #[test]
    fn pending_action_plans_as_pending() {
        let m = manifest_with(vec![ActionKind::PullImage {
            image: "img@sha256:abc".into(),
        }]);
        let p = plan(&m, &ObservedState::default()).unwrap();
        assert!(!p.is_noop());
        assert!(matches!(p.steps[0].status, StepStatus::Pending));
    }

    #[test]
    fn ensure_env_without_local_value_is_blocked() {
        let m = manifest_with(vec![ActionKind::EnsureEnv {
            keys: vec!["NEON_POSTGRES_URL".into()],
        }]);
        let p = plan(&m, &ObservedState::default()).unwrap();
        assert!(p.has_blockers());
        assert!(matches!(p.steps[0].status, StepStatus::Blocked { .. }));
    }

    #[test]
    fn ensure_env_with_local_value_is_satisfied() {
        let m = manifest_with(vec![ActionKind::EnsureEnv {
            keys: vec!["NEON_POSTGRES_URL".into()],
        }]);
        let observed = ObservedState {
            env_keys_present: vec!["NEON_POSTGRES_URL".into()],
            ..Default::default()
        };
        let p = plan(&m, &observed).unwrap();
        assert!(p.is_noop());
    }

    #[test]
    fn digest_is_stable_and_content_bound() {
        let a = manifest_with(vec![ActionKind::MigrateDatabase {
            target: "control-plane".into(),
        }]);
        let b = manifest_with(vec![ActionKind::MigrateDatabase {
            target: "other".into(),
        }]);
        assert_eq!(manifest_digest(&a).unwrap(), manifest_digest(&a).unwrap());
        assert_ne!(manifest_digest(&a).unwrap(), manifest_digest(&b).unwrap());
    }
}
