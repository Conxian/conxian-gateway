//! Per-tenant fixed-window rate limiting for the Managed SaaS Gateway surface.
//!
//! Each authenticated tenant (managed API key or Fusion JWT subject) gets an
//! independent fixed-window budget. Windows are keyed by tenant id and reset
//! every `window`. The limiter is cheap and lock-free in the hot path (a short
//! `Mutex` critical section only).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<State>>,
    limit: u32,
    window: Duration,
}

struct State {
    windows: HashMap<String, Window>,
}

struct Window {
    reset_at: Instant,
    count: u32,
}

impl RateLimiter {
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(State {
                windows: HashMap::new(),
            })),
            limit,
            window,
        }
    }

    /// Record one request for `tenant`. Returns `true` if the request is within
    /// the current window budget, `false` if the tenant has exceeded its limit.
    pub fn check(&self, tenant: &str) -> bool {
        let now = Instant::now();
        let mut state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        let w = state
            .windows
            .entry(tenant.to_string())
            .or_insert_with(|| Window {
                reset_at: now + self.window,
                count: 0,
            });

        if now >= w.reset_at {
            w.reset_at = now + self.window;
            w.count = 0;
        }

        w.count += 1;
        w.count <= self.limit
    }

    #[cfg(test)]
    fn limit(&self) -> u32 {
        self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_limit_then_blocks() {
        let limiter = RateLimiter::new(3, Duration::from_secs(60));
        assert!(limiter.check("tenant-a"));
        assert!(limiter.check("tenant-a"));
        assert!(limiter.check("tenant-a"));
        assert!(!limiter.check("tenant-a"));
    }

    #[test]
    fn tenants_are_isolated() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        assert!(limiter.check("tenant-a"));
        assert!(limiter.check("tenant-a"));
        assert!(!limiter.check("tenant-a"));
        // A different tenant still has its own budget.
        assert!(limiter.check("tenant-b"));
        assert!(limiter.check("tenant-b"));
    }

    #[test]
    fn reports_configured_limit() {
        let limiter = RateLimiter::new(42, Duration::from_secs(60));
        assert_eq!(limiter.limit(), 42);
    }
}
