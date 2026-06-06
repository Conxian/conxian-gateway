use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use std::collections::HashMap;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthRole {
    Auditor = 0,
    Operator = 1,
    Admin = 2,
}

#[derive(Clone)]
pub struct AuthStore {
    identities: Arc<HashMap<String, AuthRole>>,
}

impl AuthStore {
    pub fn new() -> Self {
        Self {
            identities: Arc::new(HashMap::new()),
        }
    }

    pub fn with_identity(mut self, token: String, role: AuthRole) -> Self {
        if !token.is_empty() && !is_sentinel(&token) {
            let mut map = (*self.identities).clone();
            map.insert(token, role);
            self.identities = Arc::new(map);
        }
        self
    }

    pub fn validate(&self, token: &str, required_role: AuthRole) -> bool {
        for (expected_token, role) in self.identities.as_ref() {
            // Constant-time comparison to prevent timing attacks
            if token.as_bytes().len() == expected_token.as_bytes().len() {
                let is_match = token
                    .as_bytes()
                    .ct_eq(expected_token.as_bytes())
                    .unwrap_u8() == 1;

                if is_match && *role >= required_role {
                    return true;
                }
            }
        }
        false
    }
}

fn is_sentinel(token: &str) -> bool {
    token == "REQUIRED_FOR_PROD_API_TOKEN" || token == "institutional-default-token"
}

pub async fn auth_middleware(
    req: Request,
    next: Next,
    store: AuthStore,
    required_role: AuthRole,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    match auth_header {
        Some(auth) if auth.starts_with("Bearer ") => {
            let provided_token = &auth[7..];
            if store.validate(provided_token, required_role) {
                Ok(next.run(req).await)
            } else {
                warn!("Unauthorized request: Invalid token or insufficient role");
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        _ => {
            warn!("Unauthorized request: Missing or malformed Bearer token");
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}
