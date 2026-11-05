# minimium-server-runtime

## Purpose / Non-goals

Owns the authoritative server lifecycle, world registry, tick, networking coordination, admission and persistence composition. It is headless and has no GPU or window role.

## Public API

`run(args)` parses server process arguments, initializes modules and startup state, then runs the authoritative runtime.

## Dependencies

Coordinates world, simulation, environment, protocol, transport, storage, platform, telemetry, assets, procgen and sample-game contracts. Astronomy and GPU crates are forbidden.

## Data flow

The process validates startup inputs, builds world instances, then applies bounded network inputs on the owner lane. Workers publish typed results back to the runtime; immutable save snapshots cross into storage.

## Ownership and execution

`ServerRuntime` owns mutable authoritative state on one lane. Worker threads do not mutate active worlds. Runtime shutdown joins workers and persists final state.

## Invariants and errors

Preserve current protocol/save compatibility, tick cadence, movement behavior, admission limits and shutdown ordering. Worker queue, protocol, storage and startup errors remain typed at their boundaries.

## Commands

Server-owned handlers compose shared command metadata. Game rules stay with their owning domain.

## Change guide

Change authoritative coordination and process lifecycle here. Reusable movement, collision, environment and storage logic belongs in their respective crates.

## Verification

Run `rtk cargo check -p minimium-server-runtime -p minimium-server` and architecture/file-length checks. Runtime acceptance is deferred.
