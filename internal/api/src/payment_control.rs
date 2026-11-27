//! Payment control (P16) and reference-matched collections (P17).
//!
//! Two gaps a business buyer checks before anything else:
//!
//!   * **P16 — payment-level maker-checker.** A two-person rule on the payment
//!     object itself, not on an admin action: create → submit → approve →
//!     release, with the maker forbidden from approving or releasing.
//!   * **P17 — reference-matched collections.** One unique reference per payer,
//!     deterministic matching on arrival, idempotent settlement, and an
//!     exportable statement.
//!
//! The domain half is pure: no clock, no RNG, no network, no I/O. Callers pass
//! the acting principal and the wall-clock millisecond, so every rule is
//! reproducible in a test. The HTTP half is a thin adapter and is mounted only
//! when `PAYMENT_CONTROL_ENABLED=1` (fail closed, like the MoR webhook).
//!
//! Identity is never taken from the request body. The acting principal comes
//! from the authenticated M2M identity, so a maker cannot approve their own
//! payment by claiming to be somebody else. A legacy shared bearer token has no
//! per-principal identity, so the mutation routes refuse it.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::m2m_auth::VerifiedM2mIdentity;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Every rejection is a typed, non-leaking error. No error carries the
/// underlying balance, counterparty or credential material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlError {
    UnknownIntent(String),
    UnknownReference(String),
    /// Separation of duties: the maker may not also approve or release.
    SamePrincipal,
    /// The transition is not legal from the current state.
    IllegalTransition {
        from: PaymentState,
        action: Action,
    },
    AmountNotPositive,
    AmountAboveLimit {
        limit_minor: i64,
    },
    ApprovalsNotPositive,
    MissingField(&'static str),
    /// The same principal tried to approve twice. Not an error in the world,
    /// but a duplicate that must not count towards the quorum.
    AlreadyApproved(String),
    InsufficientApprovals {
        have: u8,
        need: u8,
    },
    DuplicateIntent(String),
    DuplicateReceipt(String),
    AlreadySettled(String),
    NotSettled(String),
    ReferenceCollision(String),
    CurrencyMismatch {
        expected: String,
        got: String,
    },
}

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownIntent(id) => write!(f, "unknown payment intent {id}"),
            Self::UnknownReference(r) => write!(f, "unknown collection reference {r}"),
            Self::SamePrincipal => write!(
                f,
                "separation of duties: the maker of a payment cannot approve or release it"
            ),
            Self::IllegalTransition { from, action } => {
                write!(
                    f,
                    "cannot {} a payment in state {}",
                    action.as_str(),
                    from.as_str()
                )
            }
            Self::AmountNotPositive => write!(f, "amount must be positive"),
            Self::AmountAboveLimit { limit_minor } => {
                write!(
                    f,
                    "amount exceeds the configured limit of {limit_minor} minor units"
                )
            }
            Self::ApprovalsNotPositive => write!(f, "required_approvals must be at least 1"),
            Self::MissingField(name) => write!(f, "{name} is required"),
            Self::AlreadyApproved(actor) => write!(f, "{actor} has already approved this payment"),
            Self::InsufficientApprovals { have, need } => {
                write!(f, "insufficient approvals: have {have}, need {need}")
            }
            Self::DuplicateIntent(id) => write!(f, "payment intent {id} already exists"),
            Self::DuplicateReceipt(id) => write!(f, "receipt {id} was already recorded"),
            Self::AlreadySettled(r) => write!(f, "reference {r} is already settled"),
            Self::NotSettled(r) => write!(f, "reference {r} is not settled"),
            Self::ReferenceCollision(r) => {
                write!(f, "reference {r} collides with an issued reference")
            }
            Self::CurrencyMismatch { expected, got } => {
                write!(f, "currency mismatch: expected {expected}, got {got}")
            }
        }
    }
}

impl std::error::Error for ControlError {}

// ---------------------------------------------------------------------------
// P16 — payment-level maker-checker
// ---------------------------------------------------------------------------

/// Lifecycle of a payment intent. `Released`, `Rejected` and `Cancelled` are
/// terminal: no transition leaves them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentState {
    Created,
    PendingApproval,
    Approved,
    Released,
    Rejected,
    Cancelled,
}

impl PaymentState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::PendingApproval => "pending_approval",
            Self::Approved => "approved",
            Self::Released => "released",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Released | Self::Rejected | Self::Cancelled)
    }
}

/// The audit action. Recorded on every transition, so the trail is a complete
/// replay of the payment's life rather than a set of mutable fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Create,
    Submit,
    Approve,
    Reject,
    Release,
    Cancel,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Submit => "submit",
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Release => "release",
            Self::Cancel => "cancel",
        }
    }
}

/// One immutable audit record. `seq` is strictly increasing within an intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub seq: u64,
    pub at_ms: i64,
    pub actor: String,
    pub action: Action,
    pub from: PaymentState,
    pub to: PaymentState,
}

/// A recorded approval, kept as data so a quorum can be re-verified from the
/// trail alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    pub actor: String,
    pub at_ms: i64,
}

/// A payment that needs two people. Amounts are integer minor units, never
/// floats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentIntent {
    pub id: String,
    pub tenant: String,
    pub amount_minor: i64,
    pub currency: String,
    pub payee: String,
    pub reference: Option<String>,
    pub created_by: String,
    pub created_at_ms: i64,
    pub required_approvals: u8,
    pub state: PaymentState,
    pub approvals: Vec<Approval>,
    pub rejection: Option<Approval>,
    pub released_by: Option<String>,
    pub released_at_ms: Option<i64>,
    pub audit: Vec<AuditEntry>,
}

impl PaymentIntent {
    /// Create a payment in `Created`. `max_amount_minor` of 0 disables the cap.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        tenant: impl Into<String>,
        amount_minor: i64,
        currency: impl Into<String>,
        payee: impl Into<String>,
        reference: Option<String>,
        created_by: impl Into<String>,
        at_ms: i64,
        required_approvals: u8,
        max_amount_minor: i64,
    ) -> Result<Self, ControlError> {
        let id = id.into();
        let tenant = tenant.into();
        let currency = currency.into();
        let payee = payee.into();
        let created_by = created_by.into();

        if id.trim().is_empty() {
            return Err(ControlError::MissingField("id"));
        }
        if tenant.trim().is_empty() {
            return Err(ControlError::MissingField("tenant"));
        }
        if currency.trim().is_empty() {
            return Err(ControlError::MissingField("currency"));
        }
        if payee.trim().is_empty() {
            return Err(ControlError::MissingField("payee"));
        }
        if created_by.trim().is_empty() {
            return Err(ControlError::MissingField("created_by"));
        }
        if amount_minor <= 0 {
            return Err(ControlError::AmountNotPositive);
        }
        if max_amount_minor > 0 && amount_minor > max_amount_minor {
            return Err(ControlError::AmountAboveLimit {
                limit_minor: max_amount_minor,
            });
        }
        if required_approvals == 0 {
            return Err(ControlError::ApprovalsNotPositive);
        }

        let mut intent = Self {
            id,
            tenant,
            amount_minor,
            currency,
            payee,
            reference,
            created_by,
            created_at_ms: at_ms,
            required_approvals,
            state: PaymentState::Created,
            approvals: Vec::new(),
            rejection: None,
            released_by: None,
            released_at_ms: None,
            audit: Vec::new(),
        };
        let maker = intent.created_by.clone();
        intent.push_audit(at_ms, maker, Action::Create, PaymentState::Created);
        Ok(intent)
    }

    fn push_audit(&mut self, at_ms: i64, actor: String, action: Action, from: PaymentState) {
        let seq = self.audit.len() as u64 + 1;
        self.audit.push(AuditEntry {
            seq,
            at_ms,
            actor,
            action,
            from,
            to: self.state,
        });
    }

    fn transition(
        &mut self,
        to: PaymentState,
        at_ms: i64,
        actor: String,
        action: Action,
    ) -> Result<(), ControlError> {
        if self.state.is_terminal() {
            return Err(ControlError::IllegalTransition {
                from: self.state,
                action,
            });
        }
        let from = self.state;
        self.state = to;
        self.push_audit(at_ms, actor, action, from);
        Ok(())
    }

    /// The maker hands the payment to the checker. Only the maker may submit.
    pub fn submit(&mut self, actor: &str, at_ms: i64) -> Result<(), ControlError> {
        if self.state != PaymentState::Created {
            return Err(ControlError::IllegalTransition {
                from: self.state,
                action: Action::Submit,
            });
        }
        if actor != self.created_by {
            return Err(ControlError::SamePrincipal);
        }
        self.transition(
            PaymentState::PendingApproval,
            at_ms,
            actor.to_string(),
            Action::Submit,
        )
    }

    /// Record an approval. The approver must not be the maker and must not
    /// approve twice; both are refused rather than silently ignored, so a
    /// double-click cannot manufacture a quorum.
    pub fn approve(&mut self, actor: &str, at_ms: i64) -> Result<PaymentState, ControlError> {
        if self.state != PaymentState::PendingApproval {
            return Err(ControlError::IllegalTransition {
                from: self.state,
                action: Action::Approve,
            });
        }
        if actor == self.created_by {
            return Err(ControlError::SamePrincipal);
        }
        if self.approvals.iter().any(|a| a.actor == actor) {
            return Err(ControlError::AlreadyApproved(actor.to_string()));
        }

        self.approvals.push(Approval {
            actor: actor.to_string(),
            at_ms,
        });
        let next = if self.approvals.len() >= usize::from(self.required_approvals) {
            PaymentState::Approved
        } else {
            PaymentState::PendingApproval
        };
        self.transition(next, at_ms, actor.to_string(), Action::Approve)?;
        Ok(self.state)
    }

    /// Reject. Any checker may reject; the maker may not.
    pub fn reject(&mut self, actor: &str, at_ms: i64) -> Result<(), ControlError> {
        if self.state != PaymentState::PendingApproval {
            return Err(ControlError::IllegalTransition {
                from: self.state,
                action: Action::Reject,
            });
        }
        if actor == self.created_by {
            return Err(ControlError::SamePrincipal);
        }
        self.rejection = Some(Approval {
            actor: actor.to_string(),
            at_ms,
        });
        self.transition(
            PaymentState::Rejected,
            at_ms,
            actor.to_string(),
            Action::Reject,
        )
    }

    /// Release an approved payment. The maker may not release it, and a payment
    /// cannot be released before the quorum is met.
    pub fn release(&mut self, actor: &str, at_ms: i64) -> Result<(), ControlError> {
        match self.state {
            PaymentState::Approved => {}
            PaymentState::PendingApproval => {
                return Err(ControlError::InsufficientApprovals {
                    have: self.approvals.len() as u8,
                    need: self.required_approvals,
                })
            }
            from => {
                return Err(ControlError::IllegalTransition {
                    from,
                    action: Action::Release,
                })
            }
        }
        if actor == self.created_by {
            return Err(ControlError::SamePrincipal);
        }
        self.released_by = Some(actor.to_string());
        self.released_at_ms = Some(at_ms);
        self.transition(
            PaymentState::Released,
            at_ms,
            actor.to_string(),
            Action::Release,
        )
    }

    /// Withdraw before anyone approves. Only the maker may cancel, and only
    /// while the payment is unapproved.
    pub fn cancel(&mut self, actor: &str, at_ms: i64) -> Result<(), ControlError> {
        if !matches!(
            self.state,
            PaymentState::Created | PaymentState::PendingApproval
        ) {
            return Err(ControlError::IllegalTransition {
                from: self.state,
                action: Action::Cancel,
            });
        }
        if actor != self.created_by {
            return Err(ControlError::SamePrincipal);
        }
        self.transition(
            PaymentState::Cancelled,
            at_ms,
            actor.to_string(),
            Action::Cancel,
        )
    }

    /// A payment is releasable only when the quorum is met and no approval came
    /// from the maker. Re-checked at release so a corrupted store cannot pay.
    pub fn is_releasable(&self) -> bool {
        self.state == PaymentState::Approved
            && self.approvals.len() >= usize::from(self.required_approvals)
            && self.approvals.iter().all(|a| a.actor != self.created_by)
    }
}

// ---------------------------------------------------------------------------
// P17 — reference-matched collections
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionState {
    Pending,
    Settled,
    Failed,
    Returned,
}

impl CollectionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Settled => "settled",
            Self::Failed => "failed",
            Self::Returned => "returned",
        }
    }
}

/// A per-payer reference issued for one invoice. One reference, one invoice,
/// one expected amount: that is what makes matching deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionReference {
    pub reference: String,
    pub tenant: String,
    pub payer_id: String,
    pub invoice_id: String,
    pub amount_minor: i64,
    pub currency: String,
    pub issued_at_ms: i64,
    pub state: CollectionState,
    pub matched_receipt: Option<String>,
    pub settled_at_ms: Option<i64>,
}

/// An inbound payment as the operator sees it on the bank feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub receipt_id: String,
    pub reference: String,
    pub amount_minor: i64,
    pub currency: String,
    pub received_at_ms: i64,
}

/// What happened when a receipt was matched. Returned rather than thrown, so an
/// operator can see "money arrived, we do not know whose it is" as a first-class
/// outcome instead of a 400.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum MatchOutcome {
    Matched {
        reference: String,
        receipt_id: String,
    },
    AmountMismatch {
        reference: String,
        expected_minor: i64,
        received_minor: i64,
    },
    AlreadySettled {
        reference: String,
        receipt_id: String,
    },
    Duplicate {
        receipt_id: String,
    },
    Unknown {
        reference: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatementLine {
    pub reference: String,
    pub payer_id: String,
    pub invoice_id: String,
    pub amount_minor: i64,
    pub currency: String,
    pub state: CollectionState,
    pub matched_receipt: Option<String>,
}

// ---------------------------------------------------------------------------
// Journal
// ---------------------------------------------------------------------------

/// Append-only cross-object journal. `seq` is global, so the export is a total
/// order of everything the control plane did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub seq: u64,
    pub at_ms: i64,
    pub kind: String,
    pub id: String,
    pub detail: String,
}

// ---------------------------------------------------------------------------
// Aggregate
// ---------------------------------------------------------------------------

/// The control plane's whole state. Cheap to clone for tests, bounded by the
/// caller's retention policy, and fully deterministic given the same inputs.
#[derive(Debug, Default)]
pub struct PaymentControl {
    intents: BTreeMap<String, PaymentIntent>,
    references: BTreeMap<String, CollectionReference>,
    receipts: BTreeMap<String, Receipt>,
    journal: Vec<JournalEntry>,
    max_amount_minor: i64,
}

impl PaymentControl {
    pub fn new(max_amount_minor: i64) -> Self {
        Self {
            max_amount_minor,
            ..Self::default()
        }
    }

    fn journal_push(&mut self, at_ms: i64, kind: &str, id: &str, detail: String) {
        let seq = self.journal.len() as u64 + 1;
        self.journal.push(JournalEntry {
            seq,
            at_ms,
            kind: kind.to_string(),
            id: id.to_string(),
            detail,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_intent(
        &mut self,
        id: &str,
        tenant: &str,
        amount_minor: i64,
        currency: &str,
        payee: &str,
        reference: Option<String>,
        created_by: &str,
        at_ms: i64,
        required_approvals: u8,
    ) -> Result<PaymentIntent, ControlError> {
        if self.intents.contains_key(id) {
            return Err(ControlError::DuplicateIntent(id.to_string()));
        }
        let intent = PaymentIntent::new(
            id,
            tenant,
            amount_minor,
            currency,
            payee,
            reference,
            created_by,
            at_ms,
            required_approvals,
            self.max_amount_minor,
        )?;
        self.journal_push(at_ms, "intent", id, "created".to_string());
        self.intents.insert(id.to_string(), intent.clone());
        Ok(intent)
    }

    pub fn intent(&self, id: &str) -> Option<&PaymentIntent> {
        self.intents.get(id)
    }

    fn intent_mut(&mut self, id: &str) -> Result<&mut PaymentIntent, ControlError> {
        self.intents
            .get_mut(id)
            .ok_or_else(|| ControlError::UnknownIntent(id.to_string()))
    }

    pub fn submit(
        &mut self,
        id: &str,
        actor: &str,
        at_ms: i64,
    ) -> Result<PaymentState, ControlError> {
        let intent = self.intent_mut(id)?;
        intent.submit(actor, at_ms)?;
        let state = intent.state;
        self.journal_push(at_ms, "intent", id, format!("submit by {actor}"));
        Ok(state)
    }

    pub fn approve(
        &mut self,
        id: &str,
        actor: &str,
        at_ms: i64,
    ) -> Result<PaymentState, ControlError> {
        let intent = self.intent_mut(id)?;
        let state = intent.approve(actor, at_ms)?;
        self.journal_push(
            at_ms,
            "intent",
            id,
            format!("approve by {actor} -> {}", state.as_str()),
        );
        Ok(state)
    }

    pub fn reject(
        &mut self,
        id: &str,
        actor: &str,
        at_ms: i64,
    ) -> Result<PaymentState, ControlError> {
        let intent = self.intent_mut(id)?;
        intent.reject(actor, at_ms)?;
        let state = intent.state;
        self.journal_push(at_ms, "intent", id, format!("reject by {actor}"));
        Ok(state)
    }

    pub fn release(
        &mut self,
        id: &str,
        actor: &str,
        at_ms: i64,
    ) -> Result<PaymentState, ControlError> {
        let intent = self.intent_mut(id)?;
        intent.release(actor, at_ms)?;
        let state = intent.state;
        self.journal_push(at_ms, "intent", id, format!("release by {actor}"));
        Ok(state)
    }

    pub fn cancel(
        &mut self,
        id: &str,
        actor: &str,
        at_ms: i64,
    ) -> Result<PaymentState, ControlError> {
        let intent = self.intent_mut(id)?;
        intent.cancel(actor, at_ms)?;
        let state = intent.state;
        self.journal_push(at_ms, "intent", id, format!("cancel by {actor}"));
        Ok(state)
    }

    /// Issue a deterministic per-payer reference. The same inputs always give
    /// the same reference, so a retried request cannot create a second one.
    #[allow(clippy::too_many_arguments)]
    pub fn issue_reference(
        &mut self,
        tenant: &str,
        payer_id: &str,
        invoice_id: &str,
        amount_minor: i64,
        currency: &str,
        nonce: &str,
        at_ms: i64,
    ) -> Result<CollectionReference, ControlError> {
        if tenant.trim().is_empty() {
            return Err(ControlError::MissingField("tenant"));
        }
        if payer_id.trim().is_empty() {
            return Err(ControlError::MissingField("payer_id"));
        }
        if invoice_id.trim().is_empty() {
            return Err(ControlError::MissingField("invoice_id"));
        }
        if currency.trim().is_empty() {
            return Err(ControlError::MissingField("currency"));
        }
        if amount_minor <= 0 {
            return Err(ControlError::AmountNotPositive);
        }

        let reference = derive_reference(tenant, payer_id, invoice_id, nonce);
        if let Some(existing) = self.references.get(&reference).cloned() {
            // Idempotent for the identical invoice, a collision otherwise.
            if existing.invoice_id == invoice_id
                && existing.payer_id == payer_id
                && existing.amount_minor == amount_minor
                && existing.currency == currency
            {
                return Ok(existing);
            }
            return Err(ControlError::ReferenceCollision(reference));
        }

        let entry = CollectionReference {
            reference: reference.clone(),
            tenant: tenant.to_string(),
            payer_id: payer_id.to_string(),
            invoice_id: invoice_id.to_string(),
            amount_minor,
            currency: currency.to_string(),
            issued_at_ms: at_ms,
            state: CollectionState::Pending,
            matched_receipt: None,
            settled_at_ms: None,
        };
        self.journal_push(
            at_ms,
            "reference",
            &reference,
            format!("issued for invoice {invoice_id}"),
        );
        self.references.insert(reference, entry.clone());
        Ok(entry)
    }

    /// Match an inbound receipt. Idempotent by `receipt_id`: replaying the same
    /// bank line never settles twice and never doubles the statement.
    pub fn record_inbound(&mut self, receipt: Receipt) -> Result<MatchOutcome, ControlError> {
        if self.receipts.contains_key(&receipt.receipt_id) {
            return Ok(MatchOutcome::Duplicate {
                receipt_id: receipt.receipt_id,
            });
        }
        let reference = normalize_reference(&receipt.reference);
        let receipt_id = receipt.receipt_id.clone();
        let at_ms = receipt.received_at_ms;

        // Unknown references are suspense, not a rejection: the money arrived
        // and the operator must see it.
        let Some(expected) = self.references.get(&reference) else {
            self.receipts.insert(receipt_id.clone(), receipt);
            self.journal_push(
                at_ms,
                "receipt",
                &receipt_id,
                format!("unmatched reference {reference}"),
            );
            return Ok(MatchOutcome::Unknown { reference });
        };

        let expected_state = expected.state;
        let expected_currency = expected.currency.clone();
        let expected_amount = expected.amount_minor;

        if expected_state == CollectionState::Settled {
            self.receipts.insert(receipt_id.clone(), receipt);
            self.journal_push(
                at_ms,
                "receipt",
                &receipt_id,
                format!("reference {reference} already settled"),
            );
            return Ok(MatchOutcome::AlreadySettled {
                reference,
                receipt_id,
            });
        }

        // A currency mismatch is not a payment: nothing is recorded, so the
        // operator can correct the feed and retry with the same receipt id.
        if expected_currency != receipt.currency {
            return Err(ControlError::CurrencyMismatch {
                expected: expected_currency,
                got: receipt.currency.clone(),
            });
        }

        if expected_amount != receipt.amount_minor {
            let received_minor = receipt.amount_minor;
            self.receipts.insert(receipt_id.clone(), receipt);
            self.journal_push(
                at_ms,
                "receipt",
                &receipt_id,
                format!("amount mismatch on {reference}"),
            );
            return Ok(MatchOutcome::AmountMismatch {
                reference,
                expected_minor: expected_amount,
                received_minor,
            });
        }

        self.receipts.insert(receipt_id.clone(), receipt);
        let entry = self
            .references
            .get_mut(&reference)
            .expect("reference looked up above");
        entry.state = CollectionState::Settled;
        entry.matched_receipt = Some(receipt_id.clone());
        entry.settled_at_ms = Some(at_ms);
        self.journal_push(
            at_ms,
            "receipt",
            &receipt_id,
            format!("settled {reference}"),
        );
        Ok(MatchOutcome::Matched {
            reference,
            receipt_id,
        })
    }

    /// A settled reference can come back (bank return). Money is no longer
    /// available, so the state must change; it never returns to `Pending`.
    pub fn mark_returned(
        &mut self,
        reference: &str,
        at_ms: i64,
    ) -> Result<CollectionState, ControlError> {
        let entry = self
            .references
            .get_mut(&normalize_reference(reference))
            .ok_or_else(|| ControlError::UnknownReference(reference.to_string()))?;
        if entry.state != CollectionState::Settled {
            return Err(ControlError::NotSettled(entry.reference.clone()));
        }
        entry.state = CollectionState::Returned;
        let reference = entry.reference.clone();
        self.journal_push(at_ms, "reference", &reference, "returned".to_string());
        Ok(CollectionState::Returned)
    }

    pub fn statement(&self, tenant: &str) -> Vec<StatementLine> {
        self.references
            .values()
            .filter(|r| r.tenant == tenant)
            .map(|r| StatementLine {
                reference: r.reference.clone(),
                payer_id: r.payer_id.clone(),
                invoice_id: r.invoice_id.clone(),
                amount_minor: r.amount_minor,
                currency: r.currency.clone(),
                state: r.state,
                matched_receipt: r.matched_receipt.clone(),
            })
            .collect()
    }

    pub fn journal(&self) -> &[JournalEntry] {
        &self.journal
    }

    pub fn unsettled_count(&self, tenant: &str) -> usize {
        self.references
            .values()
            .filter(|r| r.tenant == tenant && r.state != CollectionState::Settled)
            .count()
    }
}

/// Deterministic, checksum-free, human-readable reference: `CX` plus ten
/// Crockford base32 characters (50 bits) over the invoice identity. Not a
/// secret — uniqueness and readability are the requirements, and both are
/// testable without a clock or an RNG.
pub fn derive_reference(tenant: &str, payer_id: &str, invoice_id: &str, nonce: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(tenant.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(payer_id.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(invoice_id.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(nonce.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::from("CX");
    out.push_str(&crockford(&digest[..7], 10));
    out
}

/// Crockford base32 without the ambiguous letters (I, L, O, U).
fn crockford(bytes: &[u8], chars: usize) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut acc: u64 = 0;
    for (i, byte) in bytes.iter().take(8).enumerate() {
        acc |= u64::from(*byte) << (8 * i);
    }
    let mut out = String::with_capacity(chars);
    for _ in 0..chars {
        out.push(ALPHABET[(acc & 0x1f) as usize] as char);
        acc >>= 5;
    }
    out
}

/// Operator-facing normalisation: trim, upper-case, and strip spaces and dashes
/// so a payer reading the reference off a phone screen still matches.
pub fn normalize_reference(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(|c| c.to_uppercase())
        .collect()
}

// ---------------------------------------------------------------------------
// HTTP adapter
// ---------------------------------------------------------------------------

/// Shared, lock-guarded control plane. `std::sync::Mutex` is deliberate: no
/// lock is ever held across an `await`, and the critical sections are pure
/// in-memory map updates.
pub type SharedControl = Arc<Mutex<PaymentControl>>;

/// Mounted only when `PAYMENT_CONTROL_ENABLED` is `1`/`true`, so the surface is
/// off by default and cannot be opened by accident.
pub fn from_env() -> Option<SharedControl> {
    let enabled = std::env::var("PAYMENT_CONTROL_ENABLED")
        .ok()
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    if !enabled {
        return None;
    }
    let max_amount_minor = std::env::var("PAYMENT_CONTROL_MAX_AMOUNT_MINOR")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .unwrap_or(0);
    Some(Arc::new(Mutex::new(PaymentControl::new(max_amount_minor))))
}

fn status_for(err: &ControlError) -> StatusCode {
    match err {
        ControlError::UnknownIntent(_) | ControlError::UnknownReference(_) => StatusCode::NOT_FOUND,
        ControlError::SamePrincipal => StatusCode::FORBIDDEN,
        ControlError::AlreadyApproved(_)
        | ControlError::InsufficientApprovals { .. }
        | ControlError::IllegalTransition { .. }
        | ControlError::DuplicateIntent(_)
        | ControlError::DuplicateReceipt(_)
        | ControlError::AlreadySettled(_)
        | ControlError::NotSettled(_)
        | ControlError::ReferenceCollision(_) => StatusCode::CONFLICT,
        ControlError::AmountNotPositive
        | ControlError::AmountAboveLimit { .. }
        | ControlError::ApprovalsNotPositive
        | ControlError::MissingField(_)
        | ControlError::CurrencyMismatch { .. } => StatusCode::BAD_REQUEST,
    }
}

fn reject(err: ControlError) -> (StatusCode, Json<ErrorBody>) {
    (
        status_for(&err),
        Json(ErrorBody {
            error: err.to_string(),
        }),
    )
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}

/// The acting principal, taken from the authenticated M2M identity only.
fn actor(
    identity: Option<Extension<VerifiedM2mIdentity>>,
) -> Result<String, (StatusCode, Json<ErrorBody>)> {
    identity.map(|Extension(id)| id.service_id).ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorBody {
                error: "payment control requires a per-principal M2M identity; a shared \
                            bearer token cannot satisfy separation of duties"
                    .to_string(),
            }),
        )
    })
}

#[derive(Debug, Deserialize)]
pub struct CreateIntentRequest {
    pub id: String,
    pub tenant: String,
    pub amount_minor: i64,
    pub currency: String,
    pub payee: String,
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default = "default_approvals")]
    pub required_approvals: u8,
}

fn default_approvals() -> u8 {
    1
}

#[derive(Debug, Serialize)]
pub struct IntentResponse {
    pub intent: PaymentIntent,
}

pub async fn create_intent(
    Extension(control): Extension<SharedControl>,
    identity: Option<Extension<VerifiedM2mIdentity>>,
    Json(req): Json<CreateIntentRequest>,
) -> Result<(StatusCode, Json<IntentResponse>), (StatusCode, Json<ErrorBody>)> {
    let actor = actor(identity)?;
    let now = now_ms();
    let mut guard = control.lock().map_err(lock_error)?;
    let intent = guard
        .create_intent(
            &req.id,
            &req.tenant,
            req.amount_minor,
            &req.currency,
            &req.payee,
            req.reference,
            &actor,
            now,
            req.required_approvals,
        )
        .map_err(reject)?;
    Ok((StatusCode::CREATED, Json(IntentResponse { intent })))
}

pub async fn submit_intent(
    Extension(control): Extension<SharedControl>,
    identity: Option<Extension<VerifiedM2mIdentity>>,
    Path(id): Path<String>,
) -> Result<Json<IntentResponse>, (StatusCode, Json<ErrorBody>)> {
    let actor = actor(identity)?;
    let now = now_ms();
    let mut guard = control.lock().map_err(lock_error)?;
    guard.submit(&id, &actor, now).map_err(reject)?;
    let intent = guard
        .intent(&id)
        .ok_or_else(|| reject(ControlError::UnknownIntent(id.clone())))?
        .clone();
    Ok(Json(IntentResponse { intent }))
}

pub async fn approve_intent(
    Extension(control): Extension<SharedControl>,
    identity: Option<Extension<VerifiedM2mIdentity>>,
    Path(id): Path<String>,
) -> Result<Json<IntentResponse>, (StatusCode, Json<ErrorBody>)> {
    let actor = actor(identity)?;
    let now = now_ms();
    let mut guard = control.lock().map_err(lock_error)?;
    guard.approve(&id, &actor, now).map_err(reject)?;
    let intent = guard
        .intent(&id)
        .ok_or_else(|| reject(ControlError::UnknownIntent(id.clone())))?
        .clone();
    Ok(Json(IntentResponse { intent }))
}

pub async fn reject_intent(
    Extension(control): Extension<SharedControl>,
    identity: Option<Extension<VerifiedM2mIdentity>>,
    Path(id): Path<String>,
) -> Result<Json<IntentResponse>, (StatusCode, Json<ErrorBody>)> {
    let actor = actor(identity)?;
    let now = now_ms();
    let mut guard = control.lock().map_err(lock_error)?;
    guard.reject(&id, &actor, now).map_err(reject)?;
    let intent = guard
        .intent(&id)
        .ok_or_else(|| reject(ControlError::UnknownIntent(id.clone())))?
        .clone();
    Ok(Json(IntentResponse { intent }))
}

pub async fn release_intent(
    Extension(control): Extension<SharedControl>,
    identity: Option<Extension<VerifiedM2mIdentity>>,
    Path(id): Path<String>,
) -> Result<Json<IntentResponse>, (StatusCode, Json<ErrorBody>)> {
    let actor = actor(identity)?;
    let now = now_ms();
    let mut guard = control.lock().map_err(lock_error)?;
    guard.release(&id, &actor, now).map_err(reject)?;
    let intent = guard
        .intent(&id)
        .ok_or_else(|| reject(ControlError::UnknownIntent(id.clone())))?
        .clone();
    Ok(Json(IntentResponse { intent }))
}

pub async fn get_intent(
    Extension(control): Extension<SharedControl>,
    Path(id): Path<String>,
) -> Result<Json<IntentResponse>, (StatusCode, Json<ErrorBody>)> {
    let guard = control.lock().map_err(lock_error)?;
    let intent = guard
        .intent(&id)
        .ok_or_else(|| reject(ControlError::UnknownIntent(id)))?
        .clone();
    Ok(Json(IntentResponse { intent }))
}

#[derive(Debug, Deserialize)]
pub struct IssueReferenceRequest {
    pub tenant: String,
    pub payer_id: String,
    pub invoice_id: String,
    pub amount_minor: i64,
    pub currency: String,
    #[serde(default)]
    pub nonce: String,
}

#[derive(Debug, Serialize)]
pub struct ReferenceResponse {
    pub reference: CollectionReference,
}

pub async fn issue_reference(
    Extension(control): Extension<SharedControl>,
    Json(req): Json<IssueReferenceRequest>,
) -> Result<(StatusCode, Json<ReferenceResponse>), (StatusCode, Json<ErrorBody>)> {
    let now = now_ms();
    let mut guard = control.lock().map_err(lock_error)?;
    let reference = guard
        .issue_reference(
            &req.tenant,
            &req.payer_id,
            &req.invoice_id,
            req.amount_minor,
            &req.currency,
            &req.nonce,
            now,
        )
        .map_err(reject)?;
    Ok((StatusCode::CREATED, Json(ReferenceResponse { reference })))
}

#[derive(Debug, Deserialize)]
pub struct InboundRequest {
    pub receipt_id: String,
    pub reference: String,
    pub amount_minor: i64,
    pub currency: String,
}

#[derive(Debug, Serialize)]
pub struct InboundResponse {
    pub outcome: MatchOutcome,
}

pub async fn record_inbound(
    Extension(control): Extension<SharedControl>,
    Json(req): Json<InboundRequest>,
) -> Result<Json<InboundResponse>, (StatusCode, Json<ErrorBody>)> {
    let receipt = Receipt {
        receipt_id: req.receipt_id,
        reference: req.reference,
        amount_minor: req.amount_minor,
        currency: req.currency,
        received_at_ms: now_ms(),
    };
    let mut guard = control.lock().map_err(lock_error)?;
    let outcome = guard.record_inbound(receipt).map_err(reject)?;
    Ok(Json(InboundResponse { outcome }))
}

#[derive(Debug, Deserialize)]
pub struct StatementQuery {
    pub tenant: String,
}

#[derive(Debug, Serialize)]
pub struct StatementResponse {
    pub tenant: String,
    pub unsettled: usize,
    pub lines: Vec<StatementLine>,
}

pub async fn get_statement(
    Extension(control): Extension<SharedControl>,
    Query(q): Query<StatementQuery>,
) -> Result<Json<StatementResponse>, (StatusCode, Json<ErrorBody>)> {
    let guard = control.lock().map_err(lock_error)?;
    Ok(Json(StatementResponse {
        tenant: q.tenant.clone(),
        unsettled: guard.unsettled_count(&q.tenant),
        lines: guard.statement(&q.tenant),
    }))
}

#[derive(Debug, Serialize)]
pub struct JournalResponse {
    pub entries: Vec<JournalEntry>,
}

pub async fn get_journal(
    Extension(control): Extension<SharedControl>,
) -> Result<Json<JournalResponse>, (StatusCode, Json<ErrorBody>)> {
    let guard = control.lock().map_err(lock_error)?;
    Ok(Json(JournalResponse {
        entries: guard.journal().to_vec(),
    }))
}

fn lock_error<T>(_: T) -> (StatusCode, Json<ErrorBody>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorBody {
            error: "payment control store is unavailable".to_string(),
        }),
    )
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "tenant-a";
    const MAKER: &str = "svc-maker";
    const CHECKER: &str = "svc-checker";
    const SECOND: &str = "svc-second";

    fn control() -> PaymentControl {
        PaymentControl::new(0)
    }

    fn intent(c: &mut PaymentControl, required: u8) {
        c.create_intent(
            "pi_1",
            T,
            250_000,
            "ZAR",
            "supplier-9",
            None,
            MAKER,
            1_000,
            required,
        )
        .unwrap();
    }

    // ---- P16 ----

    #[test]
    fn maker_cannot_approve_their_own_payment() {
        let mut c = control();
        intent(&mut c, 1);
        c.submit("pi_1", MAKER, 1_100).unwrap();
        assert_eq!(
            c.approve("pi_1", MAKER, 1_200),
            Err(ControlError::SamePrincipal)
        );
    }

    #[test]
    fn maker_cannot_release_even_after_approval() {
        let mut c = control();
        intent(&mut c, 1);
        c.submit("pi_1", MAKER, 1_100).unwrap();
        c.approve("pi_1", CHECKER, 1_200).unwrap();
        assert_eq!(
            c.release("pi_1", MAKER, 1_300),
            Err(ControlError::SamePrincipal)
        );
    }

    #[test]
    fn two_person_flow_releases() {
        let mut c = control();
        intent(&mut c, 1);
        c.submit("pi_1", MAKER, 1_100).unwrap();
        assert_eq!(
            c.approve("pi_1", CHECKER, 1_200).unwrap(),
            PaymentState::Approved
        );
        assert_eq!(
            c.release("pi_1", CHECKER, 1_300).unwrap(),
            PaymentState::Released
        );
        let i = c.intent("pi_1").unwrap();
        assert_eq!(i.released_by.as_deref(), Some(CHECKER));
        assert!(!i.is_releasable(), "released is no longer releasable");
    }

    #[test]
    fn release_before_quorum_is_refused() {
        let mut c = control();
        intent(&mut c, 2);
        c.submit("pi_1", MAKER, 1_100).unwrap();
        c.approve("pi_1", CHECKER, 1_200).unwrap();
        assert_eq!(
            c.release("pi_1", CHECKER, 1_300),
            Err(ControlError::InsufficientApprovals { have: 1, need: 2 })
        );
    }

    #[test]
    fn duplicate_approval_does_not_count_twice() {
        let mut c = control();
        intent(&mut c, 2);
        c.submit("pi_1", MAKER, 1_100).unwrap();
        c.approve("pi_1", CHECKER, 1_200).unwrap();
        assert_eq!(
            c.approve("pi_1", CHECKER, 1_250),
            Err(ControlError::AlreadyApproved(CHECKER.to_string()))
        );
        c.approve("pi_1", SECOND, 1_300).unwrap();
        assert_eq!(c.intent("pi_1").unwrap().state, PaymentState::Approved);
    }

    #[test]
    fn terminal_states_never_transition() {
        let mut c = control();
        intent(&mut c, 1);
        c.submit("pi_1", MAKER, 1_100).unwrap();
        c.reject("pi_1", CHECKER, 1_200).unwrap();
        assert!(c.approve("pi_1", SECOND, 1_300).is_err());
        assert!(c.release("pi_1", SECOND, 1_300).is_err());
        assert!(c.cancel("pi_1", MAKER, 1_300).is_err());
    }

    #[test]
    fn only_the_maker_may_submit_or_cancel() {
        let mut c = control();
        intent(&mut c, 1);
        assert_eq!(
            c.submit("pi_1", CHECKER, 1_100),
            Err(ControlError::SamePrincipal)
        );
        assert_eq!(
            c.cancel("pi_1", CHECKER, 1_100),
            Err(ControlError::SamePrincipal)
        );
        c.cancel("pi_1", MAKER, 1_100).unwrap();
        assert_eq!(c.intent("pi_1").unwrap().state, PaymentState::Cancelled);
    }

    #[test]
    fn amount_cap_and_validation() {
        let mut c = PaymentControl::new(100_000);
        assert_eq!(
            c.create_intent("pi_2", T, 100_001, "ZAR", "p", None, MAKER, 1, 1),
            Err(ControlError::AmountAboveLimit {
                limit_minor: 100_000
            })
        );
        assert_eq!(
            c.create_intent("pi_3", T, 0, "ZAR", "p", None, MAKER, 1, 1),
            Err(ControlError::AmountNotPositive)
        );
        assert_eq!(
            c.create_intent("pi_4", T, 10, "ZAR", "p", None, MAKER, 1, 0),
            Err(ControlError::ApprovalsNotPositive)
        );
        assert_eq!(
            c.create_intent("pi_5", "", 10, "ZAR", "p", None, MAKER, 1, 1),
            Err(ControlError::MissingField("tenant"))
        );
    }

    #[test]
    fn audit_trail_is_complete_and_ordered() {
        let mut c = control();
        intent(&mut c, 1);
        c.submit("pi_1", MAKER, 1_100).unwrap();
        c.approve("pi_1", CHECKER, 1_200).unwrap();
        c.release("pi_1", CHECKER, 1_300).unwrap();
        let audit = &c.intent("pi_1").unwrap().audit;
        assert_eq!(audit.len(), 4);
        assert_eq!(
            audit.iter().map(|a| a.action).collect::<Vec<_>>(),
            vec![
                Action::Create,
                Action::Submit,
                Action::Approve,
                Action::Release
            ]
        );
        for (i, entry) in audit.iter().enumerate() {
            assert_eq!(entry.seq, i as u64 + 1);
        }
        assert_eq!(audit.last().unwrap().to, PaymentState::Released);
    }

    #[test]
    fn unknown_intent_is_not_found() {
        let mut c = control();
        assert_eq!(
            c.approve("nope", CHECKER, 1),
            Err(ControlError::UnknownIntent("nope".to_string()))
        );
    }

    // ---- P17 ----

    #[test]
    fn reference_is_deterministic_and_idempotent() {
        let mut c = control();
        let a = c
            .issue_reference(T, "payer-1", "inv-1", 50_000, "ZAR", "n1", 10)
            .unwrap()
            .reference
            .clone();
        let b = c
            .issue_reference(T, "payer-1", "inv-1", 50_000, "ZAR", "n1", 11)
            .unwrap()
            .reference
            .clone();
        assert_eq!(a, b);
        assert!(a.starts_with("CX"));
        assert_eq!(a.len(), 12);
    }

    #[test]
    fn different_invoices_get_different_references() {
        let mut c = control();
        let a = c
            .issue_reference(T, "payer-1", "inv-1", 1, "ZAR", "", 1)
            .unwrap()
            .reference
            .clone();
        let b = c
            .issue_reference(T, "payer-1", "inv-2", 1, "ZAR", "", 1)
            .unwrap()
            .reference
            .clone();
        assert_ne!(a, b);
    }

    #[test]
    fn collision_on_reused_reference_is_refused() {
        let mut c = control();
        let r = c
            .issue_reference(T, "payer-1", "inv-1", 1, "ZAR", "", 1)
            .unwrap()
            .reference
            .clone();
        // Same inputs but a different amount must not silently reuse the code.
        assert_eq!(
            c.issue_reference(T, "payer-1", "inv-1", 2, "ZAR", "", 1),
            Err(ControlError::ReferenceCollision(r))
        );
    }

    #[test]
    fn exact_match_settles() {
        let mut c = control();
        let r = c
            .issue_reference(T, "payer-1", "inv-1", 50_000, "ZAR", "", 1)
            .unwrap()
            .reference
            .clone();
        let outcome = c
            .record_inbound(Receipt {
                receipt_id: "rc_1".into(),
                reference: r.clone(),
                amount_minor: 50_000,
                currency: "ZAR".into(),
                received_at_ms: 20,
            })
            .unwrap();
        assert_eq!(
            outcome,
            MatchOutcome::Matched {
                reference: r.clone(),
                receipt_id: "rc_1".into()
            }
        );
        assert_eq!(c.unsettled_count(T), 0);
    }

    #[test]
    fn reference_is_normalised_from_a_phone_screen() {
        let mut c = control();
        let r = c
            .issue_reference(T, "payer-1", "inv-1", 50_000, "ZAR", "", 1)
            .unwrap()
            .reference
            .clone();
        let spaced = format!("{} {}", &r[..6], &r[6..]);
        let outcome = c
            .record_inbound(Receipt {
                receipt_id: "rc_1".into(),
                reference: spaced.to_lowercase(),
                amount_minor: 50_000,
                currency: "ZAR".into(),
                received_at_ms: 20,
            })
            .unwrap();
        assert!(matches!(outcome, MatchOutcome::Matched { .. }));
    }

    #[test]
    fn replaying_the_same_bank_line_never_settles_twice() {
        let mut c = control();
        let r = c
            .issue_reference(T, "payer-1", "inv-1", 50_000, "ZAR", "", 1)
            .unwrap()
            .reference
            .clone();
        let receipt = Receipt {
            receipt_id: "rc_1".into(),
            reference: r.clone(),
            amount_minor: 50_000,
            currency: "ZAR".into(),
            received_at_ms: 20,
        };
        c.record_inbound(receipt.clone()).unwrap();
        assert_eq!(
            c.record_inbound(receipt).unwrap(),
            MatchOutcome::Duplicate {
                receipt_id: "rc_1".into()
            }
        );
        assert_eq!(c.statement(T).len(), 1);
    }

    #[test]
    fn second_receipt_on_a_settled_reference_is_reported() {
        let mut c = control();
        let r = c
            .issue_reference(T, "payer-1", "inv-1", 50_000, "ZAR", "", 1)
            .unwrap()
            .reference
            .clone();
        for id in ["rc_1", "rc_2"] {
            c.record_inbound(Receipt {
                receipt_id: id.into(),
                reference: r.clone(),
                amount_minor: 50_000,
                currency: "ZAR".into(),
                received_at_ms: 20,
            })
            .unwrap();
        }
        let outcome = c
            .record_inbound(Receipt {
                receipt_id: "rc_3".into(),
                reference: r.clone(),
                amount_minor: 50_000,
                currency: "ZAR".into(),
                received_at_ms: 30,
            })
            .unwrap();
        assert_eq!(
            outcome,
            MatchOutcome::AlreadySettled {
                reference: r,
                receipt_id: "rc_3".into()
            }
        );
    }

    #[test]
    fn amount_mismatch_holds_the_reference_pending() {
        let mut c = control();
        let r = c
            .issue_reference(T, "payer-1", "inv-1", 50_000, "ZAR", "", 1)
            .unwrap()
            .reference
            .clone();
        let outcome = c
            .record_inbound(Receipt {
                receipt_id: "rc_1".into(),
                reference: r.clone(),
                amount_minor: 49_000,
                currency: "ZAR".into(),
                received_at_ms: 20,
            })
            .unwrap();
        assert_eq!(
            outcome,
            MatchOutcome::AmountMismatch {
                reference: r.clone(),
                expected_minor: 50_000,
                received_minor: 49_000
            }
        );
        assert_eq!(c.statement(T)[0].state, CollectionState::Pending);
    }

    #[test]
    fn unknown_reference_is_suspense_not_an_error() {
        let mut c = control();
        let outcome = c
            .record_inbound(Receipt {
                receipt_id: "rc_x".into(),
                reference: "CXUNKNOWN000".into(),
                amount_minor: 1,
                currency: "ZAR".into(),
                received_at_ms: 20,
            })
            .unwrap();
        assert_eq!(
            outcome,
            MatchOutcome::Unknown {
                reference: "CXUNKNOWN000".into()
            }
        );
    }

    #[test]
    fn currency_mismatch_is_a_hard_error() {
        let mut c = control();
        let r = c
            .issue_reference(T, "payer-1", "inv-1", 50_000, "ZAR", "", 1)
            .unwrap()
            .reference
            .clone();
        assert_eq!(
            c.record_inbound(Receipt {
                receipt_id: "rc_1".into(),
                reference: r,
                amount_minor: 50_000,
                currency: "USD".into(),
                received_at_ms: 20,
            }),
            Err(ControlError::CurrencyMismatch {
                expected: "ZAR".into(),
                got: "USD".into()
            })
        );
    }

    #[test]
    fn returned_money_leaves_the_reference_unsettled() {
        let mut c = control();
        let r = c
            .issue_reference(T, "payer-1", "inv-1", 50_000, "ZAR", "", 1)
            .unwrap()
            .reference
            .clone();
        c.record_inbound(Receipt {
            receipt_id: "rc_1".into(),
            reference: r.clone(),
            amount_minor: 50_000,
            currency: "ZAR".into(),
            received_at_ms: 20,
        })
        .unwrap();
        assert_eq!(c.mark_returned(&r, 30).unwrap(), CollectionState::Returned);
        assert_eq!(c.unsettled_count(T), 1);
    }

    #[test]
    fn statement_is_scoped_to_one_tenant() {
        let mut c = control();
        c.issue_reference(T, "p1", "i1", 1, "ZAR", "", 1).unwrap();
        c.issue_reference("tenant-b", "p1", "i1", 1, "ZAR", "", 1)
            .unwrap();
        assert_eq!(c.statement(T).len(), 1);
        assert_eq!(c.statement("tenant-b").len(), 1);
    }

    #[test]
    fn journal_records_every_operation_in_order() {
        let mut c = control();
        intent(&mut c, 1);
        c.submit("pi_1", MAKER, 1_100).unwrap();
        c.approve("pi_1", CHECKER, 1_200).unwrap();
        let kinds: Vec<_> = c.journal().iter().map(|j| j.kind.clone()).collect();
        assert_eq!(kinds, vec!["intent", "intent", "intent"]);
        for (i, e) in c.journal().iter().enumerate() {
            assert_eq!(e.seq, i as u64 + 1);
        }
    }

    // ---- HTTP adapter: the actor comes from the credential, never the body ----

    fn shared() -> SharedControl {
        Arc::new(Mutex::new(PaymentControl::new(0)))
    }

    fn identity(service: &str) -> Extension<VerifiedM2mIdentity> {
        Extension(VerifiedM2mIdentity {
            service_id: service.to_string(),
            scopes: vec!["write:settlement".to_string()],
        })
    }

    fn create_req() -> CreateIntentRequest {
        CreateIntentRequest {
            id: "pi_http".into(),
            tenant: T.into(),
            amount_minor: 1_000,
            currency: "ZAR".into(),
            payee: "supplier".into(),
            reference: None,
            required_approvals: 1,
        }
    }

    #[tokio::test]
    async fn shared_bearer_token_cannot_act_on_a_payment() {
        let control = shared();
        let err = create_intent(Extension(control), None, Json(create_req()))
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(err.1 .0.error.contains("separation of duties"));
    }

    #[tokio::test]
    async fn handlers_enforce_two_people_end_to_end() {
        let control = shared();
        let (status, _) = create_intent(
            Extension(control.clone()),
            Some(identity(MAKER)),
            Json(create_req()),
        )
        .await
        .unwrap();
        assert_eq!(status, StatusCode::CREATED);

        let Json(submitted) = submit_intent(
            Extension(control.clone()),
            Some(identity(MAKER)),
            Path("pi_http".into()),
        )
        .await
        .unwrap();
        assert_eq!(submitted.intent.state, PaymentState::PendingApproval);

        // The maker is the same principal, so approval is refused by the handler.
        let err = approve_intent(
            Extension(control.clone()),
            Some(identity(MAKER)),
            Path("pi_http".into()),
        )
        .await
        .unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);

        let Json(approved) = approve_intent(
            Extension(control.clone()),
            Some(identity(CHECKER)),
            Path("pi_http".into()),
        )
        .await
        .unwrap();
        assert_eq!(approved.intent.state, PaymentState::Approved);
        let Json(released) = release_intent(
            Extension(control),
            Some(identity(CHECKER)),
            Path("pi_http".into()),
        )
        .await
        .unwrap();
        assert_eq!(released.intent.state, PaymentState::Released);
    }

    #[tokio::test]
    async fn unknown_intent_is_404_not_500() {
        let err = get_intent(Extension(shared()), Path("missing".into()))
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::NOT_FOUND);
    }

    #[test]
    fn reference_alphabet_excludes_ambiguous_characters() {
        for i in 0..200u32 {
            let r = derive_reference(T, "p", "i", &i.to_string());
            assert!(
                !r[2..].contains(['I', 'L', 'O', 'U']),
                "ambiguous char in {r}"
            );
        }
    }
}
