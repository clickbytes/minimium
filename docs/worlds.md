# World model and generation

Epic 003 introduces a versioned world contract shared by the server and client.
New data directories build a seeded procedural planet with a radius in the
617.5–682.5 km range and a connected dungeon. Existing revision-2 directories
retain their 10 km planet.
`minimium-server run --seed DECIMAL|0xHEX` chooses the seed for a new generated
world. Without it, the server samples a random seed once. The server records it
in `world.seed` and rejects a different seed on
later starts with the same data directory. The client obtains the seed from the
manifest in `Welcome`; the seed is visible to the client.
The server owns memberships, simulation state, sector tickets and transfers;
`procgen` is pure, `assets` validates authored definitions, and the client owns
scene, prediction and renderer resources.

Revision 3 introduces a versioned RON planet preset and stores its resolved
definition in `world.planet.ron`. Presets can use `extends` up to eight levels;
cycles and changes to an existing definition are rejected. The Earth preset
reads a separately imported GEBCO 2026 ice-surface package and uses generator
revision 4 for new directories, with a GEBCO land spawn near Bergen. Saved
resolved definitions without `generator_revision` remain revision 3; existing
coordinates, generation geometry and player positions are preserved. The default
`third_party/gebco-2026` package uses a 1-arcminute overview (about 467 MB)
streamed from the Australian Antarctic Division's COG mirror. The full
15-arcsecond netCDF import remains available; its coasts and heights are more
detailed. The Earth renderer reflects the camera's horizontal presentation so
east appears to the right in both the globe overview and first-person view.
Mouse steering, movement, and map-click rays use the same presentation; GEBCO
sampling and saved world coordinates remain unchanged. The client must have
the same package; from outside the repository
root, set `MINIMIUM_EARTH_PACKAGE` to its directory. Tile checksums and source identity
(source hash for the full package, ETag/version for the compact one) are pinned
in the world definition. Rendering
uses millimeter vertices for generated terrain and camera-relative GPU
coordinates. Active planet players use millimeter positions in simulation,
saves and snapshots; collision queries retain coarse `f32` broadphase cells.

## Identity and location

- `WorldSeed` is a 128-bit generation input. `GenerationSpec` binds it to a
  nonzero generator revision and a positive output quantum in millimeters.
- `WorldDefinitionId` identifies immutable authored content. A
  `WorldInstanceId` identifies one live simulation of that content; instances
  can contain players at the same coordinates without sharing state.
- `WorldManifest` binds definition ID, topology, generation settings, content
  revision and a 32-byte generator digest.
- Mutable time and sky configuration lives in `EnvironmentDescriptor`, outside
  the immutable manifest. The planet and interior instances share `ClockId=1`;
  each instance selects an `Interior`, `Earth` or `EarthLike` profile without
  changing generation, content revision or map fingerprints.
- `WorldLocation` carries an instance ID, signed integer millimeter position and
  canonical Q1.30 unit-quaternion orientation. Equivalent `q` and `-q` values
  canonicalize to one value.
- `MembershipEpoch`, `TransitionId`, `SectorRevision` and `ObjectOriginId` are
  typed IDs for membership, transfer and generated content ownership.

## Sector addresses

The rendered client has an `M` overview for the active world. On the default
planet it draws all six cube-sphere faces from the same deterministic seed at
coarse visual resolution; in the dungeon it draws the complete authored map.
The red marker shows the player's position. Gameplay generates collision for
addressed sectors across all six faces as players move; the overview remains a
separate visual mesh.

Planet projection v1 preserves the six face mapping used by PR #49:

| Face index | Normal | Local U | Local V |
| --- | --- | --- | --- |
| 0 | +X | +Y | +Z |
| 1 | -X | +Y | +Z |
| 2 | +Y | +X | +Z |
| 3 | -Y | +X | +Z |
| 4 | +Z | +X | +Y |
| 5 | -Z | +X | +Y |

Revision 2 uses a 256 by 256 logical sector grid per face; revision 3 uses
16,384 by 16,384. Sector adjacency crosses
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
samples. Revision 3 applies sparse mountain bands and local detail, validates output quantum and
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

Epic 004's clock contract is independent of the fixed simulation step and sector
lifecycle. `RealTime` follows server UTC; `TimeScale` advances from an explicit
UTC anchor at a nonnegative rate, with zero pausing the environment. Both modes
use the same clock evaluation and astronomical model. Physics, cooldowns and
network deadlines continue on real time. Interior instances retain the shared
clock identity even when their profile does not render the planetary sky.

The server's `WorldRegistry` owns isolated simulation state, memberships,
membership epochs and pending transfers. Portal transfer follows prepare, target
ready, then commit on a tick boundary. The server commits only after the client
has validated the target manifest and the target movement collision neighborhood
is installed. The commit moves the player to the target simulation and forces a
full target snapshot. The client checks target map fingerprint and generator
digest, then waits for target sector data, collision and GPU buffers before
acknowledging readiness. It switches its active sector scene and input state
without recreating the window, GPU device or TCP session.

Protocol v12 wraps gameplay inputs and snapshots in instance, content revision and
membership epoch. Inputs and snapshots from an older membership context are
discarded. Save v8 records every loaded world's manifest and map fingerprint,
each player's instance, millimeter position, orientation, health and return
location, revisioned sector state, and the shared world clock. v2/v3 snapshots
are attached only to a loaded legacy map with a matching fingerprint; they are
never guessed onto the procedural planet. The intermediate v4 format is accepted
only for locations in the primary instance, because it did not pin manifests for
target worlds. Every valid v2–v7 snapshot remains readable and starts a RealTime
clock at current UTC; its data is rewritten as v8 only when the next snapshot
completes.

The v8 `CLOCK` record is stored after `WORLD` records and before sector or player
records in the same atomic snapshot. RealTime uses the current UTC after restart.
TimeScale/Pause resumes `saved_world_utc_ms`; TimeScale/Advance adds
`floor(max(0, current_wall_utc_ms - saved_wall_utc_ms) * rate_ppm / 1_000_000)`
with checked arithmetic. If wall UTC moved backwards, offline advance is zero
and the server logs the saved and current values. Invalid or out-of-range clock
state aborts startup without replacing the snapshot. The process nonce and
monotonic clock are session-local and never stored.

The server and client use the same topology-specific neighborhood and geometry
generator. Both keep bounded ticketed caches. On the server, a sector becomes
installed after data and collision readiness; movement waits until every sector
in the player's neighborhood is installed. The server keeps a full immutable map
collision index for hitscan blockers. The client builds collision indices and
GPU vertex buffers from its active sector neighborhood, then replaces them as
the player moves. Planet rendering uses separate patch meshes with interior
subdivisions of 16, 8, 4 and 2 for distances below 100, 500 and 1000 meters,
and farther regions. Every patch retains the same boundary samples across LOD
levels for revision 2. Revision 3 selects quadtree patches to the computed
horizon and uses edge skirts where patch sizes differ. Portal
readiness waits for target data, collision and GPU
buffers before the client acknowledges. The client retains source maps in CPU
memory so it can create deterministic sectors locally. An existing v1 demo
snapshot is loaded with its original generator, copied to
`world.snapshot.legacy-v1`, and rewritten with positions projected along their
saved radial directions. Save v8 stores complete,
revisioned logical object-state sets by sector; the server registry checks
topology, revision order and size limits when committing or restoring them. The
demo currently has no interaction that changes sector objects. Geometry edits
and network replication of object state remain future work.

Planet movement derives local up from the radial direction, uses a tangent
movement frame and substeps collision checks. Dungeon movement keeps fixed Y-up.
Collision bounds use the corresponding capsule orientation. Workspace tests now
exercise the generated planet-to-dungeon route, portal return, save migration,
and restoration of the saved player profile. A wire-level restart/reconnect demo
also passed. A visual review of the rendered client transfer remains outstanding.
