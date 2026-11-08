# minimium-world

## Purpose / Non-goals

Owns world manifests, topology, addressing, map geometry, entities and neutral component data. Mutable player/world state, movement, collision queries and combat belong to `minimium-simulation`. This crate does not own I/O, generation workers, protocol, storage or GPU state.

## Public API

The `world` module exposes manifests, maps, topology/addressing, collision geometry and the current `World` state API. The `entity` module exposes the world-scoped component registry and entity store.

Component schemas require an exact registered version; the world crate does not migrate older component payloads. Replicated descriptors explicitly declare either a spatial position decoder or global visibility; spatial entities are filtered through active sectors, and global visibility is never inferred from a missing position. Payloads are limited to 2048 bytes, each entity to 16 components, and each store to 16,384 entities; storage/runtime enforces the 4 MiB aggregate snapshot component payload cap across worlds. Store mutations advance a monotone revision and remain available through `changes_since`; component removal and entity despawn are explicit change records. Component persistence belongs to storage/runtime composition and is not encoded into sector object blobs.

## Dependencies

Depends only on core identity values. It has no dependency on client/server adapters or generation packages.

## Data flow

Applications build world inputs from assets/procgen and hand state to runtime owners. Sector generation and persistence results return as explicit data for installation by the owner.

## Ownership and execution

Runtime consumers combine immutable world definitions and maps with simulation-owned mutable state. Geometry and addressing contracts preserve their stable units and identities.

## Invariants and errors

Preserve map fingerprints, topology addressing, geometry units and world identity.

## Commands

World inspection/creation commands execute in the host runtime. This crate defines no global command enum.

## Change guide

Change manifests, map/addressing and geometry here. Change movement, collision query, combat and mutable stepping in `minimium-simulation`.

## Verification

Run `rtk cargo check -p minimium-world` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.

PeakRelief defines a validated monotone source-height mapping (1..3x multiplier,
exponent 1..8, source start/full thresholds up to 12000 m). It uses a smoothstep
weight raised to an integer power, retaining identity below the threshold and
bounded gain above the reference height. PlanetDefinition owns the curve and
the pure Earth height/datum conversion. Identity preserves legacy arithmetic.
