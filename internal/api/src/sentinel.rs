//! Sentinel — secret filtering to prevent credential leakage.
//!
//! The Gateway server must never echo credentials back into logs, responses, or
//! error messages (BYOK / thin-orchestrator invariant). `Sentinel` holds the set
//! of live secret values for the process and redacts them from arbitrary text so
//! that logging and error paths cannot accidentally persist credentials.

use std::sync::Arc;

#[derive(Clone, Default)]
pub struct Sentinel {
    secrets: Arc<Vec<String>>,
}

impl Sentinel {
    pub fn new(secrets: impl IntoIterator<Item = String>) -> Self {
        let mut secrets: Vec<String> = secrets.into_iter().filter(|s| !s.is_empty()).collect();
        // Deduplicate to avoid redundant scans.
        secrets.sort();
        secrets.dedup();
        Self {
            secrets: Arc::new(secrets),
        }
    }

    /// Redact every registered secret value from `input`, replacing it with a
    /// fixed redaction marker that never reveals the underlying value.
    pub fn redact(&self, input: &str) -> String {
        let mut out = input.to_string();
        for secret in self.secrets.iter() {
            if out.contains(secret.as_str()) {
                out = out.replace(secret.as_str(), "[REDACTED]");
            }
        }
        out
    }

    /// Returns `true` if `input` contains any registered secret (used by tests
    /// and by the request sanitizer to decide whether to scrub headers).
    pub fn contains_secret(&self, input: &str) -> bool {
        self.secrets.iter().any(|s| input.contains(s.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_known_secrets() {
        let sentinel = Sentinel::new(vec!["sk-live-123456".to_string()]);
        let cleaned = sentinel.redact("Authorization: Bearer sk-live-123456");
        assert!(!cleaned.contains("sk-live-123456"));
        assert!(cleaned.contains("[REDACTED]"));
    }

    #[test]
    fn leaves_unrelated_text_untouched() {
        let sentinel = Sentinel::new(vec!["sk-live-123456".to_string()]);
        assert_eq!(sentinel.redact("GET /health"), "GET /health");
    }

    #[test]
    fn ignores_empty_secrets() {
        let sentinel = Sentinel::new(vec!["".to_string(), "abc".to_string()]);
        assert!(!sentinel.contains_secret(""));
        assert!(sentinel.contains_secret("abc"));
    }
}
