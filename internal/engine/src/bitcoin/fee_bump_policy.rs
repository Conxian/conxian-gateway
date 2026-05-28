use conxian_core::FeeBumpStrategy;

#[derive(Debug, Clone)]
pub struct FeeBumpPolicyConfig {
    pub stuck_threshold_secs: u64,
    pub max_attempts: u32,
    pub max_fee_rate_sat_vb: u64,
    pub min_bump_increment_sat_vb: u64,
}

impl Default for FeeBumpPolicyConfig {
    fn default() -> Self {
        Self {
            stuck_threshold_secs: 300,
            max_attempts: 3,
            max_fee_rate_sat_vb: 150,
            min_bump_increment_sat_vb: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeBumpCandidate {
    pub txid: String,
    pub first_seen_at: u64,
    pub last_bump_at: Option<u64>,
    pub bump_attempts: u32,
    pub current_fee_rate_sat_vb: u64,
    pub target_fee_rate_sat_vb: Option<u64>,
    pub rbf_allowed: bool,
    pub cpfp_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StuckClassification {
    Pending { age_secs: u64, threshold_secs: u64 },
    Stuck { age_secs: u64, threshold_secs: u64 },
}

impl StuckClassification {
    pub fn is_stuck(&self) -> bool {
        matches!(self, Self::Stuck { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoActionReason {
    NotStuck { age_secs: u64, threshold_secs: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardrailRejectReason {
    MaxAttemptsReached {
        attempts: u32,
        max_attempts: u32,
    },
    FeeCapExceeded {
        required_fee_rate_sat_vb: u64,
        max_fee_rate_sat_vb: u64,
    },
    NoAvailableStrategy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeBumpAction {
    pub strategy: FeeBumpStrategy,
    pub target_fee_rate_sat_vb: u64,
    pub fee_increment_sat_vb: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeeBumpDecision {
    NoAction { reason: NoActionReason },
    Execute(FeeBumpAction),
    Reject { reason: GuardrailRejectReason },
}

pub fn classify_stuck(
    candidate: &FeeBumpCandidate,
    now_unix_secs: u64,
    config: &FeeBumpPolicyConfig,
) -> StuckClassification {
    let baseline = candidate.last_bump_at.unwrap_or(candidate.first_seen_at);
    let age_secs = now_unix_secs.saturating_sub(baseline);

    if age_secs >= config.stuck_threshold_secs {
        StuckClassification::Stuck {
            age_secs,
            threshold_secs: config.stuck_threshold_secs,
        }
    } else {
        StuckClassification::Pending {
            age_secs,
            threshold_secs: config.stuck_threshold_secs,
        }
    }
}

pub fn decide_fee_bump(
    candidate: &FeeBumpCandidate,
    now_unix_secs: u64,
    config: &FeeBumpPolicyConfig,
) -> FeeBumpDecision {
    let classification = classify_stuck(candidate, now_unix_secs, config);

    match classification {
        StuckClassification::Pending {
            age_secs,
            threshold_secs,
        } => FeeBumpDecision::NoAction {
            reason: NoActionReason::NotStuck {
                age_secs,
                threshold_secs,
            },
        },
        StuckClassification::Stuck { .. } => decide_for_stuck(candidate, config),
    }
}

fn decide_for_stuck(candidate: &FeeBumpCandidate, config: &FeeBumpPolicyConfig) -> FeeBumpDecision {
    if candidate.bump_attempts >= config.max_attempts {
        return FeeBumpDecision::Reject {
            reason: GuardrailRejectReason::MaxAttemptsReached {
                attempts: candidate.bump_attempts,
                max_attempts: config.max_attempts,
            },
        };
    }

    let minimum_target = candidate
        .current_fee_rate_sat_vb
        .saturating_add(config.min_bump_increment_sat_vb);

    let target_fee_rate_sat_vb = candidate
        .target_fee_rate_sat_vb
        .unwrap_or(minimum_target)
        .max(minimum_target);

    if target_fee_rate_sat_vb > config.max_fee_rate_sat_vb {
        return FeeBumpDecision::Reject {
            reason: GuardrailRejectReason::FeeCapExceeded {
                required_fee_rate_sat_vb: target_fee_rate_sat_vb,
                max_fee_rate_sat_vb: config.max_fee_rate_sat_vb,
            },
        };
    }

    let strategy = if candidate.rbf_allowed {
        FeeBumpStrategy::Rbf
    } else if candidate.cpfp_allowed {
        FeeBumpStrategy::Cpfp
    } else {
        return FeeBumpDecision::Reject {
            reason: GuardrailRejectReason::NoAvailableStrategy,
        };
    };

    FeeBumpDecision::Execute(FeeBumpAction {
        strategy,
        fee_increment_sat_vb: target_fee_rate_sat_vb
            .saturating_sub(candidate.current_fee_rate_sat_vb),
        target_fee_rate_sat_vb,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> FeeBumpPolicyConfig {
        FeeBumpPolicyConfig {
            stuck_threshold_secs: 60,
            max_attempts: 3,
            max_fee_rate_sat_vb: 120,
            min_bump_increment_sat_vb: 2,
        }
    }

    fn base_candidate() -> FeeBumpCandidate {
        FeeBumpCandidate {
            txid: "parent-txid".to_string(),
            first_seen_at: 1_000,
            last_bump_at: None,
            bump_attempts: 0,
            current_fee_rate_sat_vb: 10,
            target_fee_rate_sat_vb: Some(15),
            rbf_allowed: true,
            cpfp_allowed: true,
        }
    }

    #[test]
    fn rbf_success_path_prefers_rbf_when_replaceable() {
        let config = base_config();
        let candidate = base_candidate();

        let decision = decide_fee_bump(&candidate, 1_200, &config);

        assert_eq!(
            decision,
            FeeBumpDecision::Execute(FeeBumpAction {
                strategy: FeeBumpStrategy::Rbf,
                target_fee_rate_sat_vb: 15,
                fee_increment_sat_vb: 5,
            })
        );
    }

    #[test]
    fn cpfp_fallback_path_when_rbf_is_not_possible() {
        let config = base_config();
        let mut candidate = base_candidate();
        candidate.rbf_allowed = false;
        candidate.cpfp_allowed = true;

        let decision = decide_fee_bump(&candidate, 1_200, &config);

        assert_eq!(
            decision,
            FeeBumpDecision::Execute(FeeBumpAction {
                strategy: FeeBumpStrategy::Cpfp,
                target_fee_rate_sat_vb: 15,
                fee_increment_sat_vb: 5,
            })
        );
    }

    #[test]
    fn guardrail_rejection_path_when_max_attempts_reached() {
        let config = base_config();
        let mut candidate = base_candidate();
        candidate.bump_attempts = 3;

        let decision = decide_fee_bump(&candidate, 1_200, &config);

        assert_eq!(
            decision,
            FeeBumpDecision::Reject {
                reason: GuardrailRejectReason::MaxAttemptsReached {
                    attempts: 3,
                    max_attempts: 3,
                },
            }
        );
    }
}
