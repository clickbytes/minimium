# minimium-platform

The typed admin client uses the same bounded socket exchange as legacy requests, verifies nonce and request ID, and distinguishes cancellation before sending from an unknown outcome after delivery. The stop receipt and process shutdown confirmation are separate. [Wire contract](../../docs/console-api/admin-v1.md)

## Purpose / Non-goals

Owns operating-system adapters for durable files, advisory shared/exclusive file locks, identity, endpoint files, typed atomic local-server startup status, local administrative requests, and the `AppPaths` value used by applications to pass resolved paths to runtimes. Command schemas and parsing live in `minimium-commands`; server module registration lives in `minimium-server`.

## Public API

See `src/lib.rs` for the files, identity store, server lock, endpoint, admin client, module registration and command APIs.

## Dependencies

Depends on core identifiers and `minimium-commands` for validation before local admin I/O. Domain crates must not depend on platform adapters.

## Data flow

Runtime supplies paths and typed requests; platform code performs bounded OS operations and returns values or errors.

`files::lock_file` and `files::try_lock_file` provide cross-process advisory
shared/exclusive locks; handles release ownership when dropped. Content cache
leases use shared locks, while installation and eviction acquire exclusive locks.

## Ownership and execution

Callers own lifecycle; lock and endpoint guards retain only the resources they protect and release them on drop.

## Invariants and errors

Atomic replacement and ownership-bound cleanup must remain intact. Avoid exposing secrets in diagnostics.

The identity-store adapter maps core profile-ID parse failures to
`io::ErrorKind::InvalidData` with the actual `player.profile` path. Malformed
profile files are rejected before creation or replacement; the 128-byte bound,
header, locking and atomic publication remain unchanged.

## Commands

Does not own command parsing or execution; it sends validated command text over the local administrative transport. The bounded admin response reader accepts both legacy `ERR` lines and schema v1 `ERRJSON` envelopes, preserving code, key, reason, and usage in `AdminError`.

## Change guide

Change OS-specific file or process behavior here. Move reusable command schema/parsing to `commands` with terminal, ctl and console consumers connected.

## Verification

Run `rtk cargo check -p minimium-platform` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.

The atomic write adapter exposes the failing create, write, sync, replace or parent-sync stage to preference writers. Existing callers can continue to receive the underlying I/O error directly.
