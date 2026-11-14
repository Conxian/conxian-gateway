//! Apply engine: run a plan's typed actions with per-plan consent.
//!
//! The engine never runs arbitrary code. It dispatches each typed action to an
//! [`ActionExecutor`] the caller provides, so the same engine serves the local
//! install (a real executor) and the fleet agent (a real executor reporting to
//! the server). Consent is enforced *before* any action runs: without it, the
//! engine returns the plan unapplied and untouched.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{DeviceError, DeviceResult};
use crate::manifest::ActionKind;
use crate::plan::{Plan, StepStatus};

/// Whether the local user has acknowledged this plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Consent {
    /// The user acknowledged the plan (interactive, or an explicit opt-in flag).
    Granted,
    /// No consent was recorded; nothing may be applied.
    NotGranted,
}

/// Per-action result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepOutcome {
    /// Applied, changing local state.
    Applied,
    /// Already satisfied; applying was a no-op.
    NoOp,
    /// Failed, with a reason. The engine stops at the first failure.
    Failed { reason: String },
}

/// The audit record of one apply run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub device: String,
    pub manifest_digest: String,
    pub outcomes: Vec<(ActionKind, StepOutcome)>,
    /// State keys satisfied after this run, suitable as the next `ObservedState`.
    pub satisfied_after: Vec<String>,
}

impl ApplyReport {
    pub fn all_succeeded(&self) -> bool {
        self.outcomes
            .iter()
            .all(|(_, o)| !matches!(o, StepOutcome::Failed { .. }))
    }
}

/// Executes one typed action locally and reports the outcome.
///
/// Implementors must be idempotent: applying an already-satisfied action must
/// return [`StepOutcome::NoOp`], never an error.
#[async_trait]
pub trait ActionExecutor {
    async fn execute(&self, action: &ActionKind) -> StepOutcome;
}

/// Apply a plan, honouring consent and stopping at the first failure.
///
/// * `Consent::NotGranted` → returns `Ok(None)`; nothing is applied.
/// * A blocked plan → returns `Ok(None)`; nothing is applied.
/// * Otherwise runs each action in order and returns the audit report.
pub async fn apply<E: ActionExecutor>(
    plan: &Plan,
    consent: Consent,
    executor: &E,
) -> DeviceResult<Option<ApplyReport>> {
    if consent == Consent::NotGranted {
        return Ok(None);
    }
    if plan.has_blockers() {
        // A blocked plan is a precondition failure, not a runtime error.
        return Ok(None);
    }

    let mut outcomes = Vec::with_capacity(plan.steps.len());
    let mut satisfied_after: Vec<String> = Vec::new();

    for step in &plan.steps {
        if matches!(step.status, StepStatus::Satisfied) {
            outcomes.push((step.action.clone(), StepOutcome::NoOp));
            satisfied_after.push(step.action.state_key());
            continue;
        }
        let outcome = executor.execute(&step.action).await;
        let failed = matches!(outcome, StepOutcome::Failed { .. });
        if !failed {
            satisfied_after.push(step.action.state_key());
        }
        outcomes.push((step.action.clone(), outcome));
        if failed {
            // Fail closed: do not continue past a failure.
            break;
        }
    }

    Ok(Some(ApplyReport {
        device: plan.device.clone(),
        manifest_digest: plan.manifest_digest.clone(),
        outcomes,
        satisfied_after,
    }))
}

/// A tiny guard used by callers that must reject a manifest they cannot verify.
/// Kept here so both tracks share the same refusal message.
pub fn refuse_unverified() -> DeviceError {
    DeviceError::NothingToVerify
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{plan, ObservedState};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingExecutor {
        calls: AtomicUsize,
        fail_on: Option<&'static str>,
    }

    #[async_trait]
    impl ActionExecutor for CountingExecutor {
        async fn execute(&self, action: &ActionKind) -> StepOutcome {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if Some(action.label()) == self.fail_on {
                return StepOutcome::Failed {
                    reason: "boom".into(),
                };
            }
            StepOutcome::Applied
        }
    }

    fn manifest(actions: Vec<ActionKind>) -> crate::manifest::DesiredStateManifest {
        let mut m = crate::manifest::DesiredStateManifest::new("d1", "2026-11-14T00:00:00Z");
        m.actions = actions;
        m
    }

    #[tokio::test]
    async fn without_consent_nothing_runs() {
        let m = manifest(vec![ActionKind::RestartService {
            service: "gateway".into(),
        }]);
        let p = plan(&m, &ObservedState::default()).unwrap();
        let exec = CountingExecutor {
            calls: AtomicUsize::new(0),
            fail_on: None,
        };
        let report = apply(&p, Consent::NotGranted, &exec).await.unwrap();
        assert!(report.is_none());
        assert_eq!(exec.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn satisfied_steps_do_not_call_the_executor() {
        let m = manifest(vec![ActionKind::RestartService {
            service: "gateway".into(),
        }]);
        let observed = ObservedState {
            satisfied: vec!["service:gateway".into()],
            ..Default::default()
        };
        let p = plan(&m, &observed).unwrap();
        let exec = CountingExecutor {
            calls: AtomicUsize::new(0),
            fail_on: None,
        };
        let report = apply(&p, Consent::Granted, &exec)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(exec.calls.load(Ordering::SeqCst), 0);
        assert_eq!(report.outcomes[0].1, StepOutcome::NoOp);
    }

    #[tokio::test]
    async fn blocked_plan_is_not_applied() {
        let m = manifest(vec![ActionKind::EnsureEnv {
            keys: vec!["MISSING".into()],
        }]);
        let p = plan(&m, &ObservedState::default()).unwrap();
        let exec = CountingExecutor {
            calls: AtomicUsize::new(0),
            fail_on: None,
        };
        let report = apply(&p, Consent::Granted, &exec).await.unwrap();
        assert!(report.is_none());
        assert_eq!(exec.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn failure_stops_the_run() {
        let m = manifest(vec![
            ActionKind::PullImage {
                image: "img@sha256:abc".into(),
            },
            ActionKind::RestartService {
                service: "gateway".into(),
            },
        ]);
        let p = plan(&m, &ObservedState::default()).unwrap();
        let exec = CountingExecutor {
            calls: AtomicUsize::new(0),
            fail_on: Some("pullImage"),
        };
        let report = apply(&p, Consent::Granted, &exec)
            .await
            .unwrap()
            .unwrap();
        assert!(!report.all_succeeded());
        assert_eq!(report.outcomes.len(), 1, "run must stop at first failure");
        assert_eq!(exec.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn apply_then_replan_is_a_noop() {
        // Idempotence: the satisfied_after of one run must satisfy a replan.
        let m = manifest(vec![
            ActionKind::MigrateDatabase {
                target: "control-plane".into(),
            },
            ActionKind::RestartService {
                service: "gateway".into(),
            },
        ]);
        let p1 = plan(&m, &ObservedState::default()).unwrap();
        let exec = CountingExecutor {
            calls: AtomicUsize::new(0),
            fail_on: None,
        };
        let report = apply(&p1, Consent::Granted, &exec)
            .await
            .unwrap()
            .unwrap();
        let observed = ObservedState {
            satisfied: report.satisfied_after.clone(),
            ..Default::default()
        };
        let p2 = plan(&m, &observed).unwrap();
        assert!(p2.is_noop(), "replan after apply must be a no-op");
    }
}
