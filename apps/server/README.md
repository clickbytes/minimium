# minimium-server

## Purpose / Non-goals

Provides the server process entry point, selects optional allocation instrumentation, composes sample world and combat content, and forwards command-line arguments to `minimium-server-runtime`. It does not own runtime state or GPU presentation.

## Public API

The executable entry point is `src/main.rs`. `src/composition.rs` supplies the sample world builder and combat values through the runtime's neutral composition contract.

## Dependencies

Depends on `minimium-server-runtime`, `minimium-platform`, `minimium-diagnostics`, `minimium-telemetry`, `minimium-game-sample` and the asset/world-generation contracts used by the sample builder. Telemetry is also used for process allocation instrumentation when enabled. GPU, windowing and astronomy are forbidden.

## Data flow

The entrypoint captures the process working directory once. Initial diagnostics,
configuration-failure status, and all selected data, log and content paths use
that same directory as their relative-path base.

The executable captures supported server environment settings and resolves selected data, log and content paths against the process working directory before passing configuration and world/combat composition to the runtime. Logs default to `./logs`. The child-process run identifier is supplied as `parent_run_id`. It installs the `ctrlc` termination adapter and shares a `ShutdownRequest` with the runtime; the signal callback only flips the request flag. The builder runs during runtime initialization after `world.seed` discovery, then chooses or generates a seed. Generated worlds use revision 5 and authored maps carry an authored digest source marker. Snapshot format acceptance is owned by `minimium-storage` and is updated separately from world generation.

Ctrl-C/termination, admin `server stop`, and host ownership all converge on one `ShutdownRequest`. Shutdown closes game and admin admission, rejects pending admin commands, closes client sockets, requests worker stop, captures the final immutable snapshot, waits for persistence, removes the owned endpoint, writes shutdown status, and flushes telemetry. One absolute deadline starts when lifecycle shutdown begins: worker/save/endpoint work stops at 28 seconds, while the process diagnostics writer uses the same 30-second deadline. A timed-out save or worker is an error; it does not claim that blocking OS I/O was cancelled. Final save failure reaches the executable's nonzero exit status and status file; atomic snapshot replacement preserves the previous complete snapshot.

## Ownership and execution

Generated worlds publish revision-5 manifests with topology-specific settings.
Authored world-definition manifests use the authored fingerprint marker, which
composition verifies against the materialized map before publishing the world.

The runtime crate owns mutable authoritative state. This binary owns sample content selection, process startup and global allocator selection.

## Invariants and errors

Preserve the command-line interface and exit behavior.

## Commands

No domain commands are implemented in the binary.

## Change guide

Change sample world or combat assembly here. Change server lifecycle behavior in `minimium-server-runtime`.

## Verification

Run `rtk cargo check -p minimium-server` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.
