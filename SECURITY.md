# Security Policy

## Supported Versions

The following versions of Conxian Gateway are currently being supported with security updates.

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

We take the security of Conxian Gateway seriously. If you believe you have found a security vulnerability, please report it to us by following these steps:

1. **Do not open a public issue.**
2. Send an email to security@conxian-labs.com with details of the vulnerability.
3. Include a description of the issue, steps to reproduce, and any potential impact.

We will acknowledge your report within 48 hours and provide a timeline for a fix if applicable. We request that you follow responsible disclosure practices and give us reasonable time to address the issue before making any information public.

## Incident Handling Process (Control Alignment)

When a security or control incident is suspected, responders follow this sequence:

1. **Triage (within 1 business day)**
   - Confirm incident class (security, integrity, availability, or governance-control breach).
   - Assign an incident lead from repository owners in `CODEOWNERS`.
2. **Containment**
   - Revoke or rotate exposed credentials.
   - Disable impacted webhook/provider pathways if verification cannot be trusted.
3. **Eradication and Recovery**
   - Land a reviewed fix with test coverage.
   - Run `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test` before release.
4. **Post-incident governance**
   - Record root cause, blast radius, and control gaps in release notes/changelog.
   - Track corrective actions to closure before the next production promotion.

Severity guidance:

- **SEV-1**: Active exploit, custody risk, or signature-bypass class issue.
- **SEV-2**: Material control degradation without confirmed exploit.
- **SEV-3**: Minor weakness with low immediate impact.

## Security Standards
Conxian Gateway is an institutional-grade "Compliance Pipe". We prioritize:
- **No PII Storage**: The gateway is designed to be stateless regarding user PII.
- **Cryptographic Verification**: All attestations (ZKC) are verified using industry-standard libraries (secp256k1).
- **Secure Communication**: All API endpoints must be served over TLS in production.
- **Authentication**: Mandatory Bearer token authentication for all non-public endpoints. The gateway implements constant-time token comparison to prevent timing attacks.
- **DoS Protection**: A global 10MB request body limit is enforced on all API endpoints.
- **Protected Metrics**: The `/metrics` endpoint is protected by Bearer token authentication to prevent exposure of sensitive institutional financial data.
