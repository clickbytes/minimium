# minimium-world

## Purpose / Non-goals

Owns world manifests, topology, addressing, map geometry, entities and neutral component data. Mutable player/world state, movement, collision queries and combat belong to `minimium-simulation`. This crate does not own I/O, generation workers, protocol, storage or GPU state.

## Public API

The `world` module exposes manifests, maps, topology/addressing, collision geometry and the current `World` state API. The `entity` module exposes the world-scoped component registry and entity store, plus ordered `WorldEntityChange` values for persistence adapters.

`CharacterGeometry` is the shared CPU value for feet position, the FixedY/radial
frame, 0.3 m radius, 1.7 m body height and 1.6 m eye point. Its local/world
transforms let collision, rays and presentation use the same oriented body box.
Both FixedY and radial geometry use the existing `character_frame` basis. At
FixedY and at radial +Y, that basis is right +Z/up +Y/forward +X, because both
use the common tangent calculation. The equal horizontal dimensions keep the
same axis-aligned body bounds at those orientations.

Component schemas require an exact registered version; the world crate does not migrate older component payloads. Every descriptor provides a payload validator, which the registry runs after enforcing size and schema and before creating a component blob. Insert, restore and persistence loading share this validation path; staged effect/store changes are published only after validation succeeds. Replicated descriptors explicitly declare either a spatial position decoder or global visibility; spatial entities are filtered through active sectors, and global visibility is never inferred from a missing position. Payloads are limited to 2048 bytes, each entity to 16 components, and each store to 16,384 entities; storage/runtime enforces the 4 MiB aggregate snapshot component payload cap across worlds. Store mutations advance a monotone revision and remain available through `changes_since`; component removal and entity despawn are explicit change records. Component persistence belongs to storage/runtime composition and is not encoded into sector object blobs.

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

## Persistent impact domain

`impact::BodyState` is a validated world-scoped entity component (type 0x100,
schema 1); its neutral 162-byte record is shared by storage and protocol adapters.
The impact owner registers its ServerOnly descriptor through
`impact::register_component`; server composition installs it before application
module descriptors so type/name collisions remain typed registry errors. Client
impact state travels as typed protocol messages rather than entity replication.
It carries immutable world seed, physical parameters, SI velocity, game-space
position, lifetime, fragmentation and flight/landed/collected phase. `follows`
rejects identity changes, phase regression and changes to committed craters.
`minimium_terrain::TerrainSnapshot` now owns the indexed crater field. World
retains neutral crater stamp values and the independent `RemnantSnapshot` object
view. `RemnantSnapshot` advances its independent monotone revision on insert,
update and removal; terrain and remnant revisions remain separate inputs to
procgen's `SectorContentVersion`. Removing a remnant does not mutate terrain or
advance its revision.
`terrain`, `items` and `fluid` define world-space query, inventory/action and water
observation contracts; they contain no storage or solver implementation.
Tool outcomes distinguish a blocking first hit (`Obstructed`) from a rejected
terrain/collision join (`UnsafeGeometry`) so presentation can explain the refusal.
`TerrainMaterialSet` is a validated nonempty mask of solid materials. It carries
per-action eligibility as data; `ToolKind` remains the stable inventory value and
does not decide which terrain material an item can remove.
`Inventory::ensure_items` accepts the fixed item slots supplied by application
composition, preserves idempotence and revision behavior, and rejects a grant
that would leave the equipped slot invalid.
`MapTriangle.surface` optionally carries terrain normals and six material
fractions for generated ground and excavation surfaces. Static authored geometry
leaves this field absent.
