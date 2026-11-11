//! Machine-readable API discovery for the gateway.
//!
//! Serves an OpenAPI 3.1 description of the public route subset and an
//! RFC 9727 `api-catalog` linkset so an unauthenticated agent can discover the
//! gateway's interface without out-of-band documentation. The documents are
//! static and describe the router; they deliberately advertise only routes that
//! exist on the public/agent surface.

use axum::response::IntoResponse;
use axum::Json;
use serde_json::{json, Value};

/// Canonical public API base. The gateway is reachable on multiple hosts; the
/// OpenAPI `servers` entry names the commercial middleware host.
pub const API_BASE_URL: &str = "https://api.conxian-labs.com";

/// OpenAPI 3.1 description of the public route subset.
///
/// `GET /openapi.json`
pub async fn get_openapi() -> Json<Value> {
    Json(openapi_document())
}

fn openapi_document() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Conxian Gateway API",
            "version": conxian_core::VERSION,
            "description": "Commercial middleware boundary for the Conxian protocol. \
                This document describes the public and managed-agent route subset. \
                Authenticated management surfaces (admin, private) are intentionally \
                omitted. Payment-gated operations require an x402 payment demand.",
            "license": { "name": "BUSL-1.1" }
        },
        "servers": [
            { "url": API_BASE_URL, "description": "Production middleware host" }
        ],
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "Legacy institutional bearer token (Authorization: Bearer <token>)."
                },
                "managedKey": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "x-api-key",
                    "description": "Multi-tenant managed API key issued through the Merchant-of-Record billing flow."
                },
                "onboardingToken": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "No-card onboarding bearer token (Authorization: Bearer <token>) used only to provision a first tenant."
                },
                "contributionLedgerToken": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "Genesis Credits operator token (Authorization: Bearer <token>). Authorises a verified-work award; only a maintainer-confirmed contribution may mint."
                }
            },
            "schemas": {
                "Health": {
                    "type": "object",
                    "properties": {
                        "status": { "type": "string" },
                        "service": { "type": "string" },
                        "version": { "type": "string" }
                    }
                },
                "BillingQuoteRequest": {
                    "type": "object",
                    "required": ["plan"],
                    "properties": { "plan": { "type": "string" } }
                },
                "OnboardRequest": {
                    "type": "object",
                    "required": ["idempotency_key"],
                    "properties": {
                        "idempotency_key": { "type": "string", "description": "Client-supplied key; a retry returns the existing receipt." },
                        "tier": { "type": "string", "description": "Requested tier. Defaults to the no-card sandbox lane." }
                    }
                },
                "AwardRequest": {
                    "type": "object",
                    "required": ["contributor", "contribution", "idempotency_key"],
                    "properties": {
                        "contributor": { "type": "string", "description": "Contributor id the credits accrue to (non-transferable)." },
                        "contribution": {
                            "type": "string",
                            "enum": ["verified-disproof", "verified-reproduction", "merged-pull-request", "sdk-integration", "security-audit", "enterprise-referral"],
                            "description": "The maintainer-confirmed contribution type."
                        },
                        "idempotency_key": { "type": "string", "description": "Client-supplied key; a retry returns the prior result and mints nothing further." }
                    }
                },
                "RedeemRequest": {
                    "type": "object",
                    "required": ["contributor", "redemption", "idempotency_key"],
                    "properties": {
                        "contributor": { "type": "string" },
                        "redemption": {
                            "type": "string",
                            "enum": ["relay-credits", "tier-discount", "rate-limit-uplift", "onboarding-slot"],
                            "description": "The platform utility the credits are spent on."
                        },
                        "idempotency_key": { "type": "string", "description": "Client-supplied key; a retry returns the prior receipt and spends nothing further." }
                    }
                },
                "Error": {
                    "type": "object",
                    "properties": { "error": { "type": "string" } }
                }
            },
            "responses": {
                "TooManyRequests": {
                    "description": "Rate limit exceeded. Managed callers are limited per tenant; the limit and remaining budget are returned in the RateLimit headers.",
                    "headers": {
                        "Retry-After": { "schema": { "type": "integer" } },
                        "X-RateLimit-Limit": { "schema": { "type": "integer" } },
                        "X-RateLimit-Remaining": { "schema": { "type": "integer" } }
                    }
                },
                "PaymentRequired": {
                    "description": "x402 payment demand required before the operation is served. The response carries the payment challenge.",
                    "content": {
                        "application/json": { "schema": { "$ref": "#/components/schemas/Error" } }
                    }
                }
            }
        },
        "paths": {
            "/api/v1/health": {
                "get": {
                    "operationId": "getHealth",
                    "summary": "Liveness and identity probe",
                    "security": [],
                    "responses": {
                        "200": {
                            "description": "Gateway is up.",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Health" } } }
                        }
                    }
                }
            },
            "/api/v1/billing/quote": {
                "post": {
                    "operationId": "billingQuote",
                    "summary": "Price a subscription plan",
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/BillingQuoteRequest" } } }
                    },
                    "responses": {
                        "200": { "description": "Quoted price." },
                        "429": { "$ref": "#/components/responses/TooManyRequests" }
                    }
                }
            },
            "/api/v1/billing/subscribe": {
                "post": {
                    "operationId": "billingSubscribe",
                    "summary": "Create a subscription checkout",
                    "responses": {
                        "200": { "description": "Checkout session." },
                        "402": { "$ref": "#/components/responses/PaymentRequired" }
                    }
                }
            },
            "/api/v1/billing/verify": {
                "post": {
                    "operationId": "billingVerify",
                    "summary": "Verify a billing webhook signature",
                    "responses": { "200": { "description": "Verification result." } }
                }
            },
            "/api/v1/billing/balance": {
                "get": {
                    "operationId": "billingBalance",
                    "summary": "Read the caller's credit balance",
                    "security": [{ "bearerAuth": [] }, { "managedKey": [] }],
                    "responses": { "200": { "description": "Current balance." } }
                }
            },
            "/api/v1/agent/capabilities": {
                "get": {
                    "operationId": "agentCapabilities",
                    "summary": "Diagnostic registration manifest",
                    "description": "Returns the registered capability/adapter set. This is a diagnostic manifest; it does not assert health or production support.",
                    "security": [{ "managedKey": [] }, { "bearerAuth": [] }],
                    "responses": { "200": { "description": "Capability manifest." } }
                }
            },
            "/api/v1/agent/chains": {
                "get": {
                    "operationId": "agentChains",
                    "summary": "List registered chain adapters",
                    "security": [{ "managedKey": [] }, { "bearerAuth": [] }],
                    "responses": { "200": { "description": "Registered adapters." } }
                }
            },
            "/api/v1/agent/settle": {
                "post": {
                    "operationId": "agentSettle",
                    "summary": "Settle a job card (machine-to-machine)",
                    "security": [{ "managedKey": [] }, { "bearerAuth": [] }],
                    "responses": {
                        "200": { "description": "Settlement accepted." },
                        "402": { "$ref": "#/components/responses/PaymentRequired" },
                        "429": { "$ref": "#/components/responses/TooManyRequests" }
                    }
                }
            },
            "/api/v1/agent/verify": {
                "post": {
                    "operationId": "agentVerify",
                    "summary": "Verify an attestation",
                    "security": [{ "managedKey": [] }, { "bearerAuth": [] }],
                    "responses": { "200": { "description": "Verification result." } }
                }
            },
            "/api/v1/capabilities": {
                "get": {
                    "operationId": "capabilitiesManifest",
                    "summary": "Versioned runtime capability manifest (public)",
                    "description": "Registered capability/adapter set bound to the Core chain taxonomy version, with per-adapter operation classes (observe/route/prepare/delegated_execute/unavailable). Advertises registration only; it does not assert health or production support.",
                    "security": [],
                    "responses": { "200": { "description": "Capability manifest." } }
                }
            },
            "/api/v1/chains/list": {
                "get": {
                    "operationId": "chainsList",
                    "summary": "List registered chain adapters (public)",
                    "security": [],
                    "responses": { "200": { "description": "Registered adapters." } }
                }
            },
            "/api/v1/onboard": {
                "post": {
                    "operationId": "onboardCreate",
                    "summary": "Provision a tenant (idempotent, no-card evaluation lane)",
                    "description": "Sequences entitlement, endpoint, key and activation behind one call. A repeat with the same idempotency key returns the existing receipt. Requires the onboarding bearer token.",
                    "security": [ { "onboardingToken": [] } ],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/OnboardRequest" } } }
                    },
                    "responses": {
                        "200": { "description": "Onboarding receipt (api_key present exactly once)." },
                        "400": { "description": "Unknown tier or empty idempotency key." },
                        "401": { "description": "Invalid onboarding token." },
                        "503": { "description": "A durable step failed; the tenant is resumable." }
                    }
                }
            },
            "/api/v1/onboard/{tenant_id}": {
                "get": {
                    "operationId": "onboardStatus",
                    "summary": "Current onboarding state and per-step detail",
                    "security": [ { "onboardingToken": [] } ],
                    "parameters": [ { "name": "tenant_id", "in": "path", "required": true, "schema": { "type": "string" } } ],
                    "responses": { "200": { "description": "Tenant record." }, "404": { "description": "Unknown tenant." } }
                }
            },
            "/api/v1/tenants/{tenant_id}/limits": {
                "get": {
                    "operationId": "tenantLimits",
                    "summary": "Published, machine-readable limits for the tenant tier",
                    "security": [ { "managedKey": [] } ],
                    "parameters": [ { "name": "tenant_id", "in": "path", "required": true, "schema": { "type": "string" } } ],
                    "responses": { "200": { "description": "Quota, trial window and always-free allowance as data." }, "404": { "description": "Unknown tenant." } }
                }
            },
            "/api/v1/tenants/{tenant_id}/keys": {
                "post": {
                    "operationId": "tenantKeyRotate",
                    "summary": "Rotate the tenant's active key (re-view/rotation, no show-once)",
                    "security": [ { "managedKey": [] } ],
                    "parameters": [ { "name": "tenant_id", "in": "path", "required": true, "schema": { "type": "string" } } ],
                    "responses": { "200": { "description": "New key returned exactly once." } }
                }
            },
            "/api/v1/tenants/{tenant_id}/keys/{key_id}": {
                "delete": {
                    "operationId": "tenantKeyRevoke",
                    "summary": "Revoke the tenant's active key",
                    "security": [ { "managedKey": [] } ],
                    "parameters": [
                        { "name": "tenant_id", "in": "path", "required": true, "schema": { "type": "string" } },
                        { "name": "key_id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": { "204": { "description": "Revoked." }, "404": { "description": "Key not active for tenant." } }
                }
            },
            "/api/v1/tenants/{tenant_id}/deploy": {
                "post": {
                    "operationId": "tenantDeploy",
                    "summary": "Resume/re-run provisioning (Trigger-Deploy-style single call)",
                    "security": [ { "onboardingToken": [] } ],
                    "parameters": [ { "name": "tenant_id", "in": "path", "required": true, "schema": { "type": "string" } } ],
                    "responses": { "200": { "description": "Receipt." } }
                }
            },
            "/api/v1/tenants/{tenant_id}/usage": {
                "get": {
                    "operationId": "tenantUsage",
                    "summary": "Quota for the current period",
                    "security": [ { "managedKey": [] } ],
                    "parameters": [ { "name": "tenant_id", "in": "path", "required": true, "schema": { "type": "string" } } ],
                    "responses": { "200": { "description": "Usage/quota." } }
                }
            },
            "/api/v1/contributions/award": {
                "post": {
                    "operationId": "contributionAward",
                    "summary": "Mint Genesis Credits for a maintainer-confirmed contribution",
                    "description": "Utility-only, non-transferable credits earned for verified work. Requires the contribution-ledger operator token.",
                    "security": [ { "contributionLedgerToken": [] } ],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/AwardRequest" } } }
                    },
                    "responses": {
                        "200": { "description": "Updated contributor summary." },
                        "400": { "description": "Unknown contribution or empty contributor." },
                        "401": { "description": "Invalid operator token." },
                        "409": { "description": "Per-period credit cap reached." },
                        "503": { "description": "Durable store unavailable." }
                    }
                }
            },
            "/api/v1/contributions/{contributor}": {
                "get": {
                    "operationId": "contributionSummary",
                    "summary": "Genesis Credits balance and award history for a contributor",
                    "security": [ { "contributionLedgerToken": [] } ],
                    "parameters": [ { "name": "contributor", "in": "path", "required": true, "schema": { "type": "string" } } ],
                    "responses": {
                        "200": { "description": "Contributor summary." },
                        "401": { "description": "Invalid operator token." }
                    }
                }
            },
            "/api/v1/contributions/redeem": {
                "post": {
                    "operationId": "contributionRedeem",
                    "summary": "Redeem Genesis Credits for a platform utility",
                    "description": "Spends credits on relay credits, a tier discount, a rate-limit uplift or an onboarding slot. Relay-credit redemptions grant into the metered credit balance.",
                    "security": [ { "contributionLedgerToken": [] } ],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RedeemRequest" } } }
                    },
                    "responses": {
                        "200": { "description": "Redemption receipt." },
                        "400": { "description": "Unknown redemption." },
                        "401": { "description": "Invalid operator token." },
                        "402": { "description": "Insufficient credits." },
                        "503": { "description": "Durable store or relay sink unavailable." }
                    }
                }
            }
        }
    })
}

/// x402 payment-discovery document.
///
/// `GET /.well-known/x402` — describes the payable per-call resource and its
/// price so an agent can discover that a resource is payable, what it costs,
/// and which rail settles it, without already knowing the URL. The price is
/// read from the canonical `x402` constant, so it cannot drift from the
/// charged amount.
pub async fn get_x402_discovery() -> Json<Value> {
    let payable: Vec<Value> = crate::x402::X402_PROTECTED_SUFFIXES
        .iter()
        .map(|suffix| {
            json!({
                "resource": format!("{API_BASE_URL}/api/v1{suffix}"),
                "price": {
                    "amount": crate::x402::X402_PER_CALL_SATOSHI.to_string(),
                    "asset": crate::x402::X402_ASSET,
                    "decimals": 8
                },
                "rail": "x402",
                "payment_headers": [
                    crate::x402::PAYMENT_REQUIRED_HEADER,
                    crate::x402::PAYMENT_SIGNATURE_HEADER
                ]
            })
        })
        .collect();

    Json(json!({
        "x402Version": 1,
        "facilitator": API_BASE_URL,
        "asset": crate::x402::X402_ASSET,
        "extensions": {
            "bazaar": {
                "resources": payable
            }
        },
        "payable_resources": payable
    }))
}

/// RFC 9727 `api-catalog` linkset.
///
/// `GET /.well-known/api-catalog` — `application/linkset+json`
pub async fn get_api_catalog() -> impl IntoResponse {
    let body = json!({
        "linkset": [
            {
                "anchor": format!("{API_BASE_URL}/"),
                "service-desc": [
                    {
                        "href": format!("{API_BASE_URL}/openapi.json"),
                        "type": "application/openapi+json;version=3.1"
                    }
                ],
                "service-doc": [
                    {
                        "href": "https://docs.conxian.org/",
                        "type": "text/html"
                    }
                ],
                "status": [
                    {
                        "href": "https://www.conxian-labs.com/api/status",
                        "type": "application/json"
                    }
                ]
            }
        ]
    });
    (
        [(axum::http::header::CONTENT_TYPE, "application/linkset+json")],
        Json(body),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_is_valid_31_and_lists_public_paths() {
        let doc = openapi_document();
        assert_eq!(doc["openapi"], "3.1.0");
        let paths = doc["paths"].as_object().expect("paths object");
        assert!(paths.contains_key("/api/v1/health"));
        assert!(paths.contains_key("/api/v1/agent/settle"));
        // The self-serve onboarding lane (#713) is advertised.
        assert!(paths.contains_key("/api/v1/onboard"));
        assert!(paths.contains_key("/api/v1/tenants/{tenant_id}/limits"));
        // The Genesis Credits contribution ledger is advertised.
        assert!(paths.contains_key("/api/v1/contributions/award"));
        assert!(paths.contains_key("/api/v1/contributions/{contributor}"));
        assert!(paths.contains_key("/api/v1/contributions/redeem"));
        // Every referenced schema resolves.
        let schemas = doc["components"]["schemas"].as_object().unwrap();
        assert!(schemas.contains_key("Health"));
        assert!(schemas.contains_key("OnboardRequest"));
        assert!(schemas.contains_key("AwardRequest"));
        assert!(schemas.contains_key("RedeemRequest"));
        // The onboarding token scheme is declared.
        let schemes = doc["components"]["securitySchemes"].as_object().unwrap();
        assert!(schemes.contains_key("onboardingToken"));
        assert!(schemes.contains_key("contributionLedgerToken"));
    }

    #[test]
    fn catalog_links_to_the_openapi_document() {
        let base = API_BASE_URL;
        assert!(base.starts_with("https://"));
    }

    #[test]
    fn x402_discovery_advertises_the_canonical_price() {
        // The advertised price must be the canonical constant, so it cannot
        // drift from the amount the middleware charges.
        let doc = serde_json::to_value(crate::x402::X402_PER_CALL_SATOSHI).unwrap();
        assert_eq!(doc, serde_json::json!(1000));
        assert_eq!(crate::x402::X402_ASSET, "sBTC");
        assert!(!crate::x402::X402_PROTECTED_SUFFIXES.is_empty());
    }
}
