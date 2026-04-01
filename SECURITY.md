# Security Policy

## Supported Versions

The following versions of Conxian Gateway are currently being supported with security updates.

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

We take the security of Conxian Gateway seriously. If you believe you have found a security vulnerability, please report it to us by following these steps:

1. **Do not open a public issue.**
2. Send an email to security@conxian.io with details of the vulnerability.
3. Include a description of the issue, steps to reproduce, and any potential impact.

We will acknowledge your report within 48 hours and provide a timeline for a fix if applicable. We request that you follow responsible disclosure practices and give us reasonable time to address the issue before making any information public.

## Security Standards
Conxian Gateway is an institutional-grade "Compliance Pipe". We prioritize:
- **No PII Storage**: The gateway is designed to be stateless regarding user PII.
- **Cryptographic Verification**: All attestations (ZKC) are verified using industry-standard libraries (secp256k1).
- **Secure Communication**: All API endpoints must be served over TLS in production.
- **Authentication**: Mandatory Bearer token authentication for all non-public endpoints.
