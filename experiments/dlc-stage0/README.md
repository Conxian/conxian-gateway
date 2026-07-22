# DLC Stage 0 standalone probes

These programs are isolated, bounded experiments for issue [#220](https://github.com/Conxian/conxian-gateway/issues/220).
They are **not** part of the Gateway Cargo workspace and must not be used with
real funds, a production oracle, or a production wallet. They use fixed test
keys, synthetic inputs, and no network services.

The standalone workspace pins the upstream source revisions directly:

- `rust-dlc` `v0.8.0`: [`8e6a75fbc9685e6eafa348edd45a793fcb63fa4d`](https://github.com/p2pderivatives/rust-dlc/commit/8e6a75fbc9685e6eafa348edd45a793fcb63fa4d)
- DDK `v1.1.2`: [`e0ead55870fab97510242b8d6d2a57ce1033008f`](https://github.com/bennyhodl/dlcdevkit/commit/e0ead55870fab97510242b8d6d2a57ce1033008f)
- `dlcspecs`: [`9cd9148938c616690c79d99ec6f330e213c246c5`](https://github.com/discreetlogcontracts/dlcspecs/commit/9cd9148938c616690c79d99ec6f330e213c246c5)

## Reproduction

Run these commands from the repository root. The committed lockfile contains
only registry and exact git sources; do not replace the git revisions with local
paths.

```bash
export DLC_SPECS_CHECKOUT="${TMPDIR:-/tmp}/dlcspecs-stage0"
rm -rf "$DLC_SPECS_CHECKOUT"
git clone --filter=blob:none https://github.com/discreetlogcontracts/dlcspecs.git "$DLC_SPECS_CHECKOUT"
git -C "$DLC_SPECS_CHECKOUT" checkout --detach 9cd9148938c616690c79d99ec6f330e213c246c5

cargo +1.96.0 fmt --manifest-path experiments/dlc-stage0/Cargo.toml --all -- --check
cargo +1.96.0 check --manifest-path experiments/dlc-stage0/Cargo.toml --workspace

cargo +1.96.0 run --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage0-probe
cargo +1.96.0 run --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p ddk-stage0-probe --bin ddk-stage0-probe

cargo +1.85.1 check --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe

# Expected non-zero result: ddk-messages uses the unstable
# unsigned_is_multiple_of library feature on Rust 1.85.1.
cargo +1.85.1 check --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p ddk-stage0-probe

cargo +1.96.0 run --manifest-path experiments/dlc-stage0/Cargo.toml \
  -p rust-dlc-stage0-probe --bin rust-dlc-stage0-vector-probe -- \
  --vectors "$DLC_SPECS_CHECKOUT/test/test_vectors"
```

The vector probe is intentionally caller-supplied so the specification checkout
is visible and independently verifiable. It reports seven enum/mixed schema
blocks (`localPayout` versus required `offerPayout`) and seven numerical direct
parses; six numerical vectors match offer/accept/sign bytes and the hyperbola
offer does not.

## Probe boundaries

- `rust-dlc-stage0-probe` validates a deterministic enum oracle boundary and
  constructs synthetic funding, two CETs, and a refund transaction.
- `ddk-stage0-probe` validates the enum oracle boundary, constructs two CETs and
  a refund, verifies adaptor information, signs a CET, and validates manager
  contract inputs/info.
- Neither probe runs Bitcoin Core, Electrs, a wallet, a network transport, a
  persistence layer, or a full manager offer/accept/sign session.
