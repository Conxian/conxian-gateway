# Admin API v1 Contracts: BOS Control-Plane

This document specifies the BFF (Backend-for-Frontend) and admin-facing API contracts between the BOS Control-Plane (`conxian-business`) and the Gateway Runtime (`conxian-gateway`).

## 1. Authentication
All admin endpoints require Bearer Token authentication with a token having the `admin` or `operator` role.

**Base Path:** `/admin/v1`

## 2. Release Governance

### Request Release Approval
Used by the CI/CD pipeline to register a new release candidate for approval.

- **Endpoint:** `POST /releases/request-approval`
- **Payload:** `ReleaseApprovalRequest`
- **Response:** `AdminActionResponse` (status: `pending`)

### Submit Release Decision
Used by an authorized human operator (SAB admin) to approve or reject a release.

- **Endpoint:** `POST /releases/decision`
- **Payload:** `ReleaseDecisionRequest`
- **Response:** `AdminActionResponse` (status: `approved` | `rejected`)

## 3. Governance Workflows

### Submit Governance Decision
Submits a decision on an active DAO proposal to be recorded in the BOS audit log and potentially enacted in the runtime.

- **Endpoint:** `POST /governance/decision`
- **Payload:** `GovernanceDecisionRequest`
- **Response:** `AdminActionResponse`

## 4. Environment & Registry (Future)

### Get Environment Status
- **Endpoint:** `GET /registry/status`
- **Response:** Current health and sync status of all monitored environment nodes.

### Update Registry Config
- **Endpoint:** `POST /registry/config`
- **Payload:** Key-value pairs for environment-specific settings (e.g., RPC endpoints, sync intervals).

## 5. Implementation Notes
- **Non-Executing**: These handlers register intent and record decisions in the BOS control-plane. They do not trigger immediate on-chain settlement unless explicitly documented in v2.
- **Audit Trails**: Every request generates a unique `audit_event_id` which can be used to trace the action in the BOS Audit Dashboard.
