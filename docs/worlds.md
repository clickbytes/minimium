# World model and generation

Epic 003 introduces a versioned world contract shared by the server and client.
The default demo builds the same seeded planet and separate dungeon instance.
The server owns memberships, simulation state, sector tickets and transfers;
`procgen` is pure, `assets` validates authored definitions, and the client owns
scene, prediction and renderer resources.

## Identity and location

- `WorldSeed` is a 128-bit generation input. `GenerationSpec` binds it to a
  nonzero generator revision and a positive output quantum in millimeters.
- `WorldDefinitionId` identifies immutable authored content. A
  `WorldInstanceId` identifies one live simulation of that content; instances
  can contain players at the same coordinates without sharing state.
- `WorldManifest` binds definition ID, topology, generation settings, content
  revision and a 32-byte generator digest.
- `WorldLocation` carries an instance ID, signed integer millimeter position and
  canonical Q1.30 unit-quaternion orientation. Equivalent `q` and `-q` values
  canonicalize to one value.
- `MembershipEpoch`, `TransitionId`, `SectorRevision` and `ObjectOriginId` are
  typed IDs for membership, transfer and generated content ownership.

## Sector addresses

Planet projection v1 preserves the six face mapping used by PR #49:

| Face index | Normal | Local U | Local V |
| --- | --- | --- | --- |
| 0 | +X | +Y | +Z |
| 1 | -X | +Y | +Z |
| 2 | +Y | +X | +Z |
| 3 | -Y | +X | +Z |
| 4 | +Z | +X | +Y |
| 5 | -Z | +X | +Y |

Each face has a fixed 256 by 256 logical sector grid. Sector adjacency crosses
faces using integer cube coordinates. A shared edge or corner sample maps to
one `CubeSampleKey` regardless of its face address. Changing face order, basis,
grid size or mapping rules requires a new generation revision. Sample
coordinates reserve 32,768 subdivisions per sector, so render-patch boundary
samples remain integral through LOD 15.

Dungeon sectors use signed local `(x, y, z)` coordinates. `SectorKey`
distinguishes planet and dungeon keys. `CollisionCell` uses independent types
and a separate 4096 by 4096 per-face grid for planet cells. `RenderPatchKey`
adds renderer LOD and patch coordinates without changing persistent sector
identity.

## Addressable random generation

`minimium-procgen` exposes Philox4x32-10 through a pure `AddressedRandom`. Its
address includes generation revision, output quantization, a fixed channel ID
and the full planet or dungeon sector key. `sample_u32(index)` can be requested
in any order. `object_origin(ordinal)` derives a stable nonzero
`ObjectOriginId` from the same address and a local ordinal. The address encoding
uses a versioned deterministic 128-bit hash before the Philox counter; it is not
cryptographic and IDs are not an authorization mechanism.

## Planet and dungeon geometry

The planet generator uses integer Q16 smooth value noise over canonical cube
samples. It limits relief to 1/256 of base radius, validates output quantum and
returns positions quantized in millimeters. Fixed landmarks provide a safe spawn,
an entrance and nine approach points. The default world places a portal at the
entrance and transfers into another world instance.

The procedural dungeon generator partitions a bounded grid with BSP, places
rooms and connects them with orthogonal corridors. A validated plan is capped at
128 by 128 cells and 32 rooms. Dungeon sector keys address fixed 16-unit cubes;
the floor-plan grid occupies the x/z plane at world y=0, with other vertical
sector layers empty in this revision. Authored and generated plans use the same
`DungeonBlueprint` validator and materializer.

## Authored dungeon definitions

Pass the same definition to server and client with `--world-definition PATH`:

```text
MINIMIUM_WORLD_DEFINITION 1
manifest <definition-id> dungeon <seed> <generation-revision> <quantum-mm> <content-revision> <generator-digest-hex>
dungeon <width> <height>
room <origin-id> <x> <y> <width> <height> <center-x> <center-y>
tile <x> <y> void|room|corridor
```

Every grid coordinate must have exactly one `tile` record. A plan has at least
three rooms. Each room interior must be marked `room`, its center must be a room
tile, and every walkable tile must connect to the same walkable graph. The
manifest digest must match the v1 dungeon generator contract for the listed
generation settings. The parser bounds input to 1 MiB and validates dimensions,
room bounds, centers, origin IDs and connectivity before materialization.

## Runtime ownership and transfer

The server's `WorldRegistry` owns isolated simulation state, memberships,
membership epochs and pending transfers. Portal transfer follows prepare, target
ready, then commit on a tick boundary. The server commits only after the client
has validated the target manifest and the target movement collision neighborhood
is installed. The commit moves the player to the target simulation and forces a
full target snapshot. The client checks target map fingerprint and generator
digest, then waits for target sector data, collision and GPU buffers before
acknowledging readiness. It switches its active sector scene and input state
without recreating the window, GPU device or TCP session.

Protocol v9 wraps gameplay inputs and snapshots in instance, content revision and
membership epoch. Inputs and snapshots from an older membership context are
discarded. Save v6 records every loaded world's manifest and map fingerprint,
along with each player's instance, millimeter position, orientation, health and
return location. v2/v3 snapshots are attached only to a loaded legacy map with a
matching fingerprint; they are never guessed onto the procedural planet. The
intermediate v4 format is accepted only for locations in the primary instance,
because it did not pin manifests for target worlds.

The server and client use the same topology-specific neighborhood and geometry
slicer. Both keep bounded ticketed caches. On the server, a sector becomes
installed after data and collision readiness; movement waits until every sector
in the player's neighborhood is installed. The server keeps a full immutable map
collision index for hitscan blockers. The client builds collision indices and
GPU vertex buffers from its active sector neighborhood, then replaces them as
the player moves. Portal readiness waits for target data, collision and GPU
buffers before the client acknowledges. The client retains source maps in CPU
memory so it can create deterministic slices locally. Save v6 stores complete,
revisioned logical object-state sets by sector; the server registry checks
topology, revision order and size limits when committing or restoring them. The
demo currently has no interaction that changes sector objects. Geometry edits
and network replication of object state remain future work.

Planet movement derives local up from the radial direction, uses a tangent
movement frame and substeps collision checks. Dungeon movement keeps fixed Y-up.
Collision bounds use the corresponding capsule orientation. Workspace tests now
exercise the generated planet-to-dungeon route, portal return, save migration,
and restoration of the saved player profile. The rendered client transfer and a
restart/reconnect demo still need manual review.
