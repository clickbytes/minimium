# minimium-platform

## Purpose / Non-goals

Owns operating-system adapters for durable files, identity, locks, endpoint files and local administrative requests. Command schemas and parsing live in `minimium-commands`; server module registration lives in `minimium-server`.

## Public API

See `src/lib.rs` for the files, identity store, server lock, endpoint, admin client, module registration and command APIs.

## Dependencies

Depends on core identifiers and `minimium-commands` for validation before local admin I/O. Domain crates must not depend on platform adapters.

## Data flow

Runtime supplies paths and typed requests; platform code performs bounded OS operations and returns values or errors.

## Ownership and execution

Callers own lifecycle; lock and endpoint guards retain only the resources they protect and release them on drop.

## Invariants and errors

Atomic replacement and ownership-bound cleanup must remain intact. Avoid exposing secrets in diagnostics.

## Commands

Does not own command parsing or execution; it sends validated command text over the local administrative transport.

## Change guide

Change OS-specific file or process behavior here. Move reusable command schema/parsing to `commands` with terminal, ctl and console consumers connected.

## Verification

Run `rtk cargo check -p minimium-platform` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.
