# minimium-simulation

## Purpose / Non-goals

Owns mutable player state, movement integration, collision indexing and queries, combat, world snapshots and fixed-step results. It does not perform I/O, generation, protocol encoding or GPU work.

## Public API

`World` owns mutable simulation state; `CharacterInput`, `MovementSettings` and `step_character` provide the shared server/prediction movement path. `step_batch` processes immutable batches, while `CollisionQueryStats` reports bounded query work.

## Dependencies

Consumes world maps, geometry and manifests from `minimium-world`, plus core player identifiers. The world domain does not depend on simulation.

## Data flow

Runtime supplies input and an explicit delta; simulation returns state/events or batch results. Workers read immutable collision data, and the owning runtime applies results in order.

## Ownership and execution

Server runtime owns authoritative `World` instances. Client runtime owns predictive state. Both call the same movement integrator; workers never mutate active worlds.

## Invariants and errors

Preserve fixed-step duration, movement range, gravity/collision algorithms, deterministic batch merge order and combat rules.

## Commands

Movement and combat declarations belong to this domain; host adapters own dispatch.

## Change guide

Change movement, collision or combat rules here. Keep geometry definitions and topology addressing in `minimium-world`.

## Verification

Run `rtk cargo check -p minimium-simulation -p minimium-server -p minimium-client -p minimium-protocol` and architecture/file-length checks. Runtime acceptance is deferred.
