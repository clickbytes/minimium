# ADR 0002: Keep simulation ownership explicit and persistence off the tick path

- Status: Accepted
- Scope: Server execution and player snapshot storage

## Context

Optional parallel work must not create competing owners of mutable world state. Save formatting, synchronization, and atomic replacement are also work that should not be performed inline with the simulation tick.

## Decision

The main server lane owns and mutates the authoritative world. The configured lane count includes that main lane: one lane executes work inline, while additional lanes are persistent workers for selected independent jobs. Work is joined at explicit merge points; worker threads do not directly own mutable world state.

The executor belongs to the server. Core prepares reusable movement batches and immutable collision geometry through `World::step_players_with`; the dispatcher returns processed batches in input order and core applies them before releasing the mutable world borrow. The server executor preserves submission order when collecting worker results. One lane or fewer than 32 players uses the existing inline path; parallel batches target 16 players each.

Player snapshot work is sent through a bounded request channel to a dedicated persistence worker. The worker serializes and writes captured snapshots outside the simulation loop. Manual save, autosave, and controlled shutdown use this persistence path. Controlled shutdown reports the final save outcome.

Lane-equivalent authoritative outcomes are a requirement to verify, not a result established by this decision. Parallel work should be retained only when repeatable measurements show its benefit and account for synchronization cost.

## Consequences

- Lane count is a configuration input, not a promise that every server operation is parallel.
- Persistence has an explicit queue boundary and completion/error path.
- The bounded request channel limits queued save work; a busy persistence path can reject another request rather than grow without bound.
- Runtime and performance comparisons must report configured lanes and actual process threads.

## Evidence boundary

The current code contains an executor, thresholded movement batches, and a persistence worker. This ADR does not claim deterministic equivalence across lane counts, a particular save latency, or a performance gain. Those require deferred runtime verification and measurement.

The persistence worker uses a bounded request channel and typed save status, preserving the existing status text. A disconnected completion channel with a pending save is reported as failure. Final shutdown waits for the worker and propagates failure; a prior runtime error remains primary and subsequent cleanup errors are reported separately. An interrupted or unfinished save may still be lost on a crash.
