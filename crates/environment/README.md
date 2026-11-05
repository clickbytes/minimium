# minimium-environment

## Purpose / Non-goals

Owns world clock algorithms and values, environment descriptors, renderer-independent sky observer contracts, the bounded solar-system definition catalogue and deterministic weather generation. It contains no FFI, GPU, camera or renderer implementation.

## Public API

`time` exports clock values and deterministic clock operations; the crate root exports environment descriptors; `sky` exports observer and sky-model contracts; `universe` exports catalog identifiers, definitions and validation; `weather` exports climate/cloud models driven by a host-provided `TerrainSampler`.

## Dependencies

Depends on core values and public world manifest/definition types. It is independent of astronomy FFI and presentation adapters.

## Data flow

World manifest and observer inputs produce validated observations, catalog lookups and weather snapshots. Hosts supply monotonic/system samples and terrain queries.

## Ownership and execution

Clock values and weather models are deterministic; callers provide monotonic/system samples and a terrain adapter explicitly. The client owns astronomy calculators, GPU resources and worker lifetimes.

## Invariants and errors

Preserve coordinate conventions, catalogue bounds, model revisions and explicit invalid-observer errors.

## Commands

Owns time commands and pure UTC/time-scale argument parsing. Server runtime performs command dispatch and authorization checks.

## Change guide

Change clock, observer, catalogue and weather models here. Implement `TerrainSampler` at the host boundary; keep GPU and astronomy FFI in their adapters.

## Verification

Run `rtk cargo check -p minimium-environment` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.
