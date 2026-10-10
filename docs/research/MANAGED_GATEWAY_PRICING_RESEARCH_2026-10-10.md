# Managed Gateway Pricing — Industry Research & Unification Recommendation

**Date:** 2026-10-10
**Status:** Recommendation (owner decision required before the Rust billing engine is changed)
**Owner:** Botshelo Mokoka (Founder)

## 1. Problem

The managed SaaS Gateway is advertised at two conflicting prices:

| Surface | Base | Metered |
| :--- | :--- | :--- |
| `conxian-labs-site/server.js` (`/api/pricing/managed`) | **$99/mo** | $0.001 / call |
| `conxian-gateway/internal/engine/src/billing.rs` | **$200/mo** | $0.01 / relay, $0.05 / RWA verification, $0.10 / settlement |

The gateway billing engine is the **actual revenue surface** (what a customer is charged). The labs-site is the onboarding surface. Advertising a price the billing engine does not charge is a broken promise to subscribers.

## 2. Industry benchmarks

Managed crypto/relay/API infrastructure converges on a **hybrid** model: a monthly base, metered per-operation fees, volume tiers/discounts, and enterprise committed-use contracts.

| Provider | Category | Model | Price points |
| :--- | :--- | :--- | :--- |
| **Voltage** | Managed Lightning infra | Transaction-only (no commitment) vs Unlimited (fixed licensing fee) | Was ~$20–40/mo/node; now custom enterprise |
| **Lightspark** | Lightning payments | Free base + % of volume, then enterprise committed-use | Starter $0 + 0.50% (≤$300k/mo); Enterprise $108k–$270k/yr + 0.15–0.30% |
| **OpenNode** | Bitcoin payment processor | Pure % of value, no monthly | 1% flat; first $1,500/mo free |
| **Blockdaemon** | Enterprise node infra | Monthly base | $199/mo Starter |
| **GetBlock** | Node-as-a-service API | Free tier + metered | Free (20 RPS); dedicated $1,000–$1,500/mo |
| **QuickNode** | Multi-chain RPC API | Base + per-credit overage | Free tier; overage ~$0.50–0.62 per million credits |
| **Stripe (card ref)** | Payment processing | % + flat per transaction | 2.9% + 30¢ |

**Key takeaways:**

1. **Managed infrastructure** (the category the Gateway belongs to) prices a **flat monthly base + metered per-operation** — not a pure % of value (that is the *payment-processor* category, e.g. OpenNode/Strike).
2. **Volume tiers / discounts** are the standard enterprise lever (Lightspark 0.50% → 0.15%, Gateway already has 20% at 100k relay messages).
3. A **free or cheap entry tier** drives developer adoption (GetBlock/QuickNode free tier, OpenNode's first-$1,500-free).
4. Per-request fees in the infrastructure category cluster around **fractions of a cent** (QuickNode ~$0.0000005–0.0000006 per credit), while per-*operation* fees (settlement, verification) are higher because they carry custody/attestation cost.

## 3. Recommended canonical model (single source of truth)

**The gateway billing engine (`billing.rs`) is the source of truth.** The labs-site must mirror its constants, never advertise a conflicting price.

Proposed tier ladder (reconciles the existing $99 vs $200 by making them distinct tiers instead of one conflicting price):

| Tier | Base | Metered | Target |
| :--- | :--- | :--- | :--- |
| **Indie** | $99/mo | $0.001 / relay message | Indie AI-agent developers; low volume; no RWA/settlement metering |
| **Managed** | $200/mo | $0.01 / relay · $0.05 / RWA verification · $0.10 / settlement | Production gateway (matches `billing.rs` today) |
| **Enterprise** | Committed-use | Volume discounts (20% @ 100k messages already implemented) | Institutional / contract |

This is the **hybrid best approach**: subscription base + metered usage + volume tiers + enterprise committed-use — exactly the pattern Voltage/Lightspark/Blockdaemon use.

## 4. Options to move

1. **Align labs-site to gateway (done, 2026-10-10).** `server.js` `MANAGED_TIER` now mirrors `billing.rs` constants ($200 + $0.01/$0.05/$0.10). Zero engine risk; resolves the inconsistency immediately.
2. **Add an `Indie` tier to the billing engine.** Introduce `GatewayTier` (Indie/Managed/Enterprise) + `INDIE_BASE_FEE_CENTS = 9_900` and `INDIE_RELAY_COST_CENTS` to `billing.rs`, then re-expose the $99 price on labs-site as a *distinct* tier. Recommended when the $99 entry price is a product requirement.
3. **Reconsider the $200 base against Blockdaemon's $199 Starter.** The current Managed base is at the *enterprise* price point; a lower entry base with higher metering may convert more indie developers.
4. **Ratify pricing in writing** (`.github-private` KB) so marketing and billing cannot drift again; add a CI conformance check that asserts labs-site constants equal gateway constants.

## 5. Follow-ups

- [ ] Owner decision: keep a single $200 Managed tier, or add Indie/Enterprise tiers (Options 2/3).
- [ ] Add CI conformance test binding labs-site ↔ gateway pricing constants (Option 4).
- [ ] Wire `MERCHANT_CHECKOUT_URL` (Lemon Squeezy/Paddle) so onboarding actually charges the canonical amount.

---

*Sources: Voltage vs Lightspark (voltage.cloud), Voltage vs OpenNode (voltage.cloud), Bitcoin Node-as-a-Service comparison (spark.money), Speed vs OpenNode (tryspeed.com), OpenNode announcement (medium.com), Amboss Lightning API guide (amboss.tech).*

*This document was authored by an AI agent (OpenHands) on behalf of the maintainers.*
