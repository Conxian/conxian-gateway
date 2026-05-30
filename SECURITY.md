# Security Policy

## Supported Versions

The latest maintained release line receives security updates.

| Version | Supported |
| ------- | --------- |
| 0.1.x | ✅ |

## Reporting a Vulnerability

Do **not** open a public issue for security vulnerabilities.

Instead, report privately using one of these channels:

1. GitHub private vulnerability reporting for this repository.
2. Email [security@conxian-labs.com](mailto:security@conxian-labs.com).

Please include:

- a clear description of the issue
- steps to reproduce or a proof of concept
- potential impact
- suggested remediation, if known

We aim to acknowledge reports within 48 hours and will coordinate remediation and disclosure responsibly.

## Incident handling

When a security or control incident is suspected:

1. triage the incident and assign an owner
2. contain exposure and rotate affected secrets if needed
3. land a reviewed fix with tests
4. document follow-up actions in the changelog or release notes

## Security expectations

- no public disclosure before coordinated remediation
- no real secrets committed to source control
- production endpoints must use TLS
- protected endpoints require authenticated access
