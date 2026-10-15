# TLS dependency remediation

The 2026-10-14 RustSec snapshot reports four Rustls advisories (RUSTSEC-2026-0346/0347/0348/0349) with a patched floor of 0.23.46, plus QUIC advisories patched by quinn-proto 0.11.18. The lockfile updates those current branches without changing request/signing interfaces.

The optional native RGB path activates bp-esplora → minreq 2.14.1 → rustls 0.21.12. The legacy branch has no compatible patched 0.21 release in the advisory range. bp-esplora exposes an upstream `blocking-https-native` feature mapping to minreq's `https-native` transport. This feature preserves HTTPS with certificate and hostname validation instead of retaining the old Rustls backend. Other current Rustls consumers stay on 0.23.46.

The native connector uses native-tls defaults, including certificate/hostname validation and TLS 1.2 minimum. No invalid-certificate or invalid-hostname bypass is enabled. The existing Debian Docker image already installs libssl3 and ca-certificates; its builder installs libssl-dev and pkg-config. Deployments using native RGB must maintain OS TLS/CA updates and trust-store policy. This is a transport dependency change, not provider attestation or production custody qualification.

Primary references: [bp-esplora source](https://github.com/BP-WG/bp-esplora-client), [minreq HTTPS features](https://docs.rs/minreq/2.14.1/minreq/), [native-tls connector defaults](https://docs.rs/native-tls/latest/native_tls/struct.TlsConnectorBuilder.html), and the RustSec advisory database at commit `caee11f41fab07d676c0a4655a2a5dd1f94da97c`.
