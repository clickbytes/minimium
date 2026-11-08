# minimium-sectors

## Purpose / Non-goals

Owns bounded sector request queues, generation tickets, readiness, installation, pinning, dirtiness and eviction. It does not generate sectors, perform I/O or upload GPU resources.

## Public API

The public manager and ticket/state/result types are defined in `src/lib.rs`. Consumers request work, begin it with a ticket, then resolve it with `complete`, `fail`, or `cancel`. Tickets bind the world instance, sector, generation revision, content revision, and unique task serial. Consumers inspect readiness and manage pins.

## Dependencies

Depends on `minimium-core` sector and revision identifiers.

## Data flow

Request → ticket → external generation → ticket-checked install → ready payload → explicit eviction/release.

## Ownership and execution

The manager is mutable state owned by its calling runtime lane. Workers return results; they do not mutate the manager.

## Invariants and errors

Stale tickets cannot install results. Failed entries can be requested again, while cancellation removes queued or active tickets so their cache and queue capacity can be reused. Generating, dirty or pinned entries cannot be evicted. A late result for a cancelled or replaced task is stale.

## Commands

None.

## Change guide

Change lifecycle transitions here. Keep generation policy in procgen and rendering installation in the client.

## Verification

Run `rtk cargo check -p minimium-sectors` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.
