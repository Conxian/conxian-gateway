# Repo ownership

## Purpose

`conxian-gateway` is the canonical integration and adapter layer for Bitcoin mainnet and Bitcoin-connected layers in the Conxian builder platform.

## This repo owns

- network and provider adapters
- observation and broadcast service boundaries
- interoperability and bridge logic
- integration surfaces for Bitcoin mainnet, Lightning, Stacks, and future supported layers

## This repo does not own

- canonical shared-core ownership
- wallet UX
- portfolio-wide strategy documents
- consumer product positioning

## Boundary rule

If logic is adapter-specific, provider-specific, or layer-runtime-specific, it should live here rather than in `lib-conxian-core`, `conxius-wallet`, or `Conxian`.

## Strategic role

Primary strategic repo.