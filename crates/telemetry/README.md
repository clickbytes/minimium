# minimium-telemetry

## Purpose / Non-goals

Owns duration metrics, process sampling and optional allocation instrumentation. It does not own domain state or decide gameplay behavior.

## Public API

Duration, process and allocation facilities are exposed from `src/lib.rs`; allocation instrumentation is feature-gated.

## Dependencies

Independent of other workspace packages so it can be used without creating domain dependency cycles.

## Data flow

Runtime and instrumentation call sites submit measurements; telemetry exposes bounded summaries and process samples.

## Ownership and execution

Metric stores are owned by the collecting runtime; allocation accounting uses its documented thread-local mechanism.

## Invariants and errors

Keep collection bounded and optional instrumentation disabled unless explicitly enabled.

## Commands

No domain commands.

## Change guide

Change measurement representation here; avoid adding telemetry dependencies to core world algorithms.

## Verification

Run `rtk cargo check -p minimium-telemetry` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.
