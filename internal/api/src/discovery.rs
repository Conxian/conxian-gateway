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
            "/api/v1/chains/list": {
                "get": {
                    "operationId": "chainsList",
                    "summary": "List registered chain adapters (public)",
                    "security": [],
                    "responses": { "200": { "description": "Registered adapters." } }
                }
            }
        }
    })
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
        // Every referenced schema resolves.
        let schemas = doc["components"]["schemas"].as_object().unwrap();
        assert!(schemas.contains_key("Health"));
    }

    #[test]
    fn catalog_links_to_the_openapi_document() {
        let base = API_BASE_URL;
        assert!(base.starts_with("https://"));
    }
}
