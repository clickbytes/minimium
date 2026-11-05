# minimium-server

## Purpose / Non-goals

Provides the server process entry point, selects optional allocation instrumentation and forwards command-line arguments to `minimium-server-runtime`. It does not own runtime state or GPU presentation.

## Public API

The executable entry point is `src/main.rs`; runtime composition and `run(args)` belong to `minimium-server-runtime`.

## Dependencies

Depends on `minimium-server-runtime` and optionally `minimium-telemetry` for process allocation instrumentation. GPU, windowing and astronomy are forbidden.

## Data flow

The executable passes process arguments to the runtime entry point and reports startup/runtime errors.

## Ownership and execution

The runtime crate owns mutable authoritative state. This binary owns only process startup and the global allocator selection.

## Invariants and errors

Preserve the command-line interface and exit behavior.

## Commands

No domain commands are implemented in the binary.

## Change guide

Change argument forwarding or allocator selection here. Change server lifecycle behavior in `minimium-server-runtime`.

## Verification

Run `rtk cargo check -p minimium-server` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.
