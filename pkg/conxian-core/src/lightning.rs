use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// SRL-1: Failure Taxonomy for Lightning Payments.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FailureTaxonomy {
    /// Failure that will not resolve with retries (e.g., No Route, Invalid Invoice).
    Permanent,
    /// Failure that may resolve with retries (e.g., Timeout, Temporary Channel Failure).
    Transient,
    /// Failure where the outcome is unknown (e.g., In-flight Handoff, Backend Crash).
    Indeterminate,
}

impl fmt::Display for FailureTaxonomy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Permanent => write!(f, "PERMANENT"),
            Self::Transient => write!(f, "TRANSIENT"),
            Self::Indeterminate => write!(f, "INDETERMINATE"),
        }
    }
}

/// SRL-1: Payment Lifecycle State Machine.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaymentLifecycle {
    /// Payment intent created, not yet sent to backend.
    #[default]
    Created,
    /// Payment sent to backend, waiting for result.
    Pending,
    /// Payment is being routed through the network.
    Routing,
    /// Payment successfully settled.
    Settled,
    /// Payment failed definitely.
    Failed,
    /// Payment is stuck in an indeterminate state and requires manual recovery or watchtower intervention.
    Stuck,
}

impl fmt::Display for PaymentLifecycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Error, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum PaymentStateError {
    #[error("Invalid transition from {from} to {to}")]
    InvalidTransition {
        from: PaymentLifecycle,
        to: PaymentLifecycle,
    },
}

impl PaymentLifecycle {
    /// Validates if a transition to a new state is permitted.
    pub fn validate_transition(&self, next: PaymentLifecycle) -> Result<(), PaymentStateError> {
        match (self, next) {
            (Self::Created, Self::Pending) => Ok(()),
            (Self::Pending, Self::Routing)
            | (Self::Pending, Self::Settled)
            | (Self::Pending, Self::Failed)
            | (Self::Pending, Self::Stuck) => Ok(()),
            (Self::Routing, Self::Settled)
            | (Self::Routing, Self::Failed)
            | (Self::Routing, Self::Stuck) => Ok(()),
            (Self::Stuck, Self::Settled) | (Self::Stuck, Self::Failed) => Ok(()),
            // Self-transitions or backward transitions generally disallowed except specific recovery paths
            (s1, s2) if s1 == &s2 => Ok(()),
            _ => Err(PaymentStateError::InvalidTransition {
                from: *self,
                to: next,
            }),
        }
    }
}

/// SRL-1: Payment Intent Model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentIntent {
    pub intent_id: String,
    pub challenge: String,
    pub amount_msat: u64,
    pub asset: String,
    pub expiry: u64,
    pub state: PaymentLifecycle,
    pub failure_reason: Option<FailureTaxonomy>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub retry_count: u32,
    pub metadata: Option<serde_json::Value>,
}

impl PaymentIntent {
    pub fn new(
        intent_id: String,
        challenge: String,
        amount_msat: u64,
        asset: String,
        expiry: u64,
    ) -> Self {
        let now = Utc::now();
        Self {
            intent_id,
            challenge,
            amount_msat,
            asset,
            expiry,
            state: PaymentLifecycle::Created,
            failure_reason: None,
            last_error: None,
            created_at: now,
            updated_at: now,
            retry_count: 0,
            metadata: None,
        }
    }

    pub fn transition(&mut self, next: PaymentLifecycle) -> Result<(), PaymentStateError> {
        self.state.validate_transition(next)?;
        self.state = next;
        self.updated_at = Utc::now();
        Ok(())
    }
}

/// SRL-1: Payment Event Model for audit trails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentEvent {
    pub event_id: String,
    pub intent_id: String,
    pub from_state: PaymentLifecycle,
    pub to_state: PaymentLifecycle,
    pub timestamp: DateTime<Utc>,
    pub detail: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payment_state_transitions() {
        let mut intent = PaymentIntent::new(
            "test-1".to_string(),
            "lnbc1...".to_string(),
            1000,
            "BTC".to_string(),
            12345678,
        );

        assert_eq!(intent.state, PaymentLifecycle::Created);

        intent.transition(PaymentLifecycle::Pending).unwrap();
        assert_eq!(intent.state, PaymentLifecycle::Pending);

        intent.transition(PaymentLifecycle::Routing).unwrap();
        assert_eq!(intent.state, PaymentLifecycle::Routing);

        intent.transition(PaymentLifecycle::Settled).unwrap();
        assert_eq!(intent.state, PaymentLifecycle::Settled);

        // Invalid transition
        let result = intent.transition(PaymentLifecycle::Created);
        assert!(result.is_err());
    }

    #[test]
    fn test_stuck_recovery_transitions() {
        let mut intent = PaymentIntent::new(
            "test-stuck".to_string(),
            "lnbc1...".to_string(),
            1000,
            "BTC".to_string(),
            12345678,
        );
        intent.transition(PaymentLifecycle::Pending).unwrap();
        intent.transition(PaymentLifecycle::Stuck).unwrap();

        // Stuck can go to Settled or Failed
        assert!(intent.clone().transition(PaymentLifecycle::Settled).is_ok());
        assert!(intent.clone().transition(PaymentLifecycle::Failed).is_ok());
        // Stuck cannot go back to Pending
        assert!(intent
            .clone()
            .transition(PaymentLifecycle::Pending)
            .is_err());
    }

    #[test]
    fn test_failure_taxonomy_serialization() {
        let fail = FailureTaxonomy::Permanent;
        let json = serde_json::to_string(&fail).unwrap();
        assert_eq!(json, "\"PERMANENT\"");
    }

    #[test]
    fn test_display_implementations() {
        assert_eq!(format!("{}", FailureTaxonomy::Permanent), "PERMANENT");
        assert_eq!(format!("{}", FailureTaxonomy::Transient), "TRANSIENT");
        assert_eq!(format!("{}", FailureTaxonomy::Indeterminate), "INDETERMINATE");

        assert_eq!(format!("{}", PaymentLifecycle::Created), "Created");
        assert_eq!(format!("{}", PaymentLifecycle::Pending), "Pending");
    }

    #[test]
    fn test_payment_event_model() {
        let event = PaymentEvent {
            event_id: "e1".into(),
            intent_id: "i1".into(),
            from_state: PaymentLifecycle::Created,
            to_state: PaymentLifecycle::Pending,
            timestamp: Utc::now(),
            detail: Some("details".into()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: PaymentEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.event_id, "e1");
    }

    #[test]
    fn test_payment_state_error_display() {
        let err = PaymentStateError::InvalidTransition {
            from: PaymentLifecycle::Settled,
            to: PaymentLifecycle::Created,
        };
        assert!(format!("{}", err).contains("Invalid transition"));
    }
}
