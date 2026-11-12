//! Measured fee-policy inputs (Gateway #531).
//!
//! Core owns the *pure* fee model (`lib_conxian_core::fee`): the load-factor
//! curve, rail cost estimates and floors. This module owns the **gateway-side
//! measured input** that feeds that model — an observed mempool fee percentile
//! with an explicit freshness window — so network I/O and provider
//! configuration never live in `lib-conxian-core` (Gateway #531 acceptance).
//!
//! A missing, stale or unparsable observation yields an explicit
//! `unavailable`/`stale` state; the caller then falls back to the configured
//! (owner-approved) floor rather than treating a default as measured data.

use lib_conxian_core::fee::{load_factor_from_mempool_percentile, LoadOracle};
use serde::{Deserialize, Serialize};

/// Default freshness window (seconds) for a mempool observation. Beyond this the
/// observation is reported stale and the load factor is not applied.
pub const DEFAULT_MEMPOOL_FRESHNESS_SECONDS: u64 = 60;

/// The explicit availability of a measured input. A default is never reported
/// as `Observed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasuredInputState {
    /// A fresh observation was captured within the freshness window.
    Observed,
    /// An observation exists but is older than the freshness window.
    Stale,
    /// No observation is available (provider absent or failed).
    Unavailable,
}

/// A measured mempool-congestion observation, versioned by unit and source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MempoolLoadObservation {
    /// Version of this measured-input binding.
    pub schema_version: u32,
    /// Fee percentile (0–100) of the observed mempool fee distribution.
    pub fee_percentile: f64,
    /// Unix seconds when the observation was captured.
    pub observed_at_unix: u64,
    /// Provenance of the observation (e.g. `bitcoin-core:getblockstats`).
    pub source: String,
    /// The unit the percentile is expressed in.
    pub unit: MeasuredInputUnit,
}

/// The unit a measured input is expressed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasuredInputUnit {
    FeePercentile,
}

/// Current schema version of the measured mempool-input binding.
pub const MEMPOOL_INPUT_SCHEMA_VERSION: u32 = 1;

impl MempoolLoadObservation {
    /// Build an observation, tagging it with the current schema version and unit.
    pub fn new(fee_percentile: f64, observed_at_unix: u64, source: impl Into<String>) -> Self {
        Self {
            schema_version: MEMPOOL_INPUT_SCHEMA_VERSION,
            fee_percentile,
            observed_at_unix,
            source: source.into(),
            unit: MeasuredInputUnit::FeePercentile,
        }
    }

    /// Classify this observation against `now` and the freshness window.
    ///
    /// An out-of-range percentile or a future timestamp is `Unavailable` (a
    /// malformed observation must not become a congestion surcharge), a
    /// timestamp older than the window is `Stale`, otherwise `Observed`.
    pub fn state_at(&self, now: u64, freshness_seconds: u64) -> MeasuredInputState {
        if !(0.0..=100.0).contains(&self.fee_percentile) || self.observed_at_unix > now {
            return MeasuredInputState::Unavailable;
        }
        if now.saturating_sub(self.observed_at_unix) > freshness_seconds {
            return MeasuredInputState::Stale;
        }
        MeasuredInputState::Observed
    }
}

/// A gateway [`LoadOracle`] driven by a measured mempool observation.
///
/// When the observation is `Observed`, [`LoadOracle::load_factor`] returns the
/// Core curve value for the observed percentile; when it is `Stale` or
/// `Unavailable`, it returns `fallback_factor` — the configured (owner-approved)
/// floor — never a fabricated congestion surcharge.
#[derive(Debug, Clone)]
pub struct MeasuredLoadOracle {
    observation: MempoolLoadObservation,
    freshness_seconds: u64,
    fallback_factor: f64,
    now: u64,
}

impl MeasuredLoadOracle {
    pub fn new(
        observation: MempoolLoadObservation,
        freshness_seconds: u64,
        fallback_factor: f64,
        now: u64,
    ) -> Self {
        Self {
            observation,
            freshness_seconds,
            fallback_factor,
            now,
        }
    }

    /// The classified state of the underlying observation at `now`.
    pub fn state(&self) -> MeasuredInputState {
        self.observation.state_at(self.now, self.freshness_seconds)
    }
}

impl LoadOracle for MeasuredLoadOracle {
    fn load_factor(&self) -> f64 {
        match self.state() {
            MeasuredInputState::Observed => {
                load_factor_from_mempool_percentile(self.observation.fee_percentile)
            }
            MeasuredInputState::Stale | MeasuredInputState::Unavailable => self.fallback_factor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(percentile: f64, at: u64) -> MempoolLoadObservation {
        MempoolLoadObservation::new(percentile, at, "bitcoin-core:getblockstats")
    }

    #[test]
    fn fresh_observation_uses_the_core_curve() {
        let o = obs(95.0, 1_000);
        assert_eq!(o.state_at(1_000, 60), MeasuredInputState::Observed);
        let oracle = MeasuredLoadOracle::new(o, 60, 1.0, 1_000);
        // 95th percentile → 2.5 on the ADR-004 curve.
        assert!((oracle.load_factor() - 2.5).abs() < 1e-9);
    }

    #[test]
    fn low_percentile_is_no_congestion() {
        let oracle = MeasuredLoadOracle::new(obs(10.0, 1_000), 60, 1.0, 1_000);
        assert_eq!(oracle.load_factor(), 1.0);
    }

    #[test]
    fn stale_observation_falls_back_to_configured_floor() {
        let oracle = MeasuredLoadOracle::new(obs(99.0, 1_000), 60, 1.0, 1_061);
        assert_eq!(oracle.state(), MeasuredInputState::Stale);
        assert_eq!(oracle.load_factor(), 1.0);
    }

    #[test]
    fn missing_observation_falls_back_and_never_invents_load() {
        // A future timestamp is malformed → Unavailable.
        let oracle = MeasuredLoadOracle::new(obs(80.0, 2_000), 60, 1.0, 1_000);
        assert_eq!(oracle.state(), MeasuredInputState::Unavailable);
        assert_eq!(oracle.load_factor(), 1.0);
    }

    #[test]
    fn out_of_range_percentile_is_unavailable() {
        assert_eq!(
            obs(150.0, 1_000).state_at(1_000, 60),
            MeasuredInputState::Unavailable
        );
        assert_eq!(
            obs(-5.0, 1_000).state_at(1_000, 60),
            MeasuredInputState::Unavailable
        );
    }

    #[test]
    fn fallback_factor_is_respected_when_unavailable() {
        let oracle = MeasuredLoadOracle::new(obs(99.0, 1_000), 60, 1.5, 10_000);
        assert_eq!(oracle.load_factor(), 1.5);
    }
}
