# World model and generation

The current world contract is shared by the server and both client frontends.
New data directories build a seeded procedural planet with a radius in the
617.5–682.5 km range and a connected dungeon. Generated manifests require
generator revision 5; other generated revisions are rejected rather than
reinterpreted. Authored worlds use their explicit content fingerprint.
`minimium-server run --seed DECIMAL|0xHEX` chooses the seed for a new generated
world. Without it, the server samples a random seed once. The server records it
in `world.seed` and rejects a different seed on
later starts with the same data directory. The client obtains the seed from the
manifest in `Welcome`; the seed is visible to the client.
The server owns memberships, simulation state, sector tickets and transfers;
`procgen` is pure, `assets` validates authored definitions, and the client owns
scene, prediction and renderer resources.

The versioned RON planet preset is resolved and stored in `world.planet.ron`.
Presets can use `extends` up to eight levels; cycles and changes to an existing
definition are rejected. The Earth preset
reads a separately imported GEBCO 2026 ice-surface package and uses generator
revision 5 for new directories, with a GEBCO land spawn near Bergen. Saved
resolved definitions carry the generator revision and digest; unsupported
generated definitions fail validation. The default
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

The protocol v18 client estimator uses one local monotonic epoch per
`ClientSession` and exposes one immutable `EnvironmentFrameInput` after polling
and portal installation. Its date, active descriptor, context, clock revision,
rate and synchronization quality are shared by renderer environment consumers.
Quality progresses through Initializing, Synchronized, Stale and Unavailable;
network sessions never fall back to local wall-clock time. Only explicit
`--demo` mode uses a separate local UTC anchor, reported as `LocalDemo`. Starting
a network attempt disables that demo source; reconnect always starts with a new
session nonce, offset filter and smoothing state.

The server's `WorldRegistry` owns isolated simulation state, memberships,
membership epochs and pending transfers. Portal transfer follows prepare, target
ready, then commit on a tick boundary. The server commits only after the client
has validated the target manifest and the target movement collision neighborhood
is installed. The commit moves the player to the target simulation and forces a
full target snapshot. The client checks target map fingerprint and generator
digest, then waits for target sector data, collision and GPU buffers before
acknowledging readiness. It switches its active sector scene and input state
without recreating the window, GPU device or TCP session.

Protocol v18 wraps gameplay inputs and snapshots in instance, content revision and
membership epoch. Inputs and snapshots from an older membership context are
discarded. Save v10 records every loaded world's manifest and map fingerprint,
each player's instance, millimeter position, orientation, health and return
location, revisioned sector state, world-clock state, and persistent entity
records including entity counters and content-initialization markers. Older and
unknown save versions are rejected before a persistence worker starts; no
migration is provided. Use a separate data directory when moving older data to
the current format.

The v10 `CLOCK` record is stored after `WORLD` records and before sector, entity
or player records in the same atomic snapshot. RealTime uses the current UTC after restart.
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
memory so it can create deterministic sectors locally. Save v10 stores complete, revisioned logical
object-state sets by sector and persistent component entities. Entity mutation
reserves storage budget before publication; the server captures the complete
entity set for autosave and shutdown. Protocol v18 synchronizes bounded entity
replicas through a per-client journal and applies full/delta images atomically
on the client. Component schemas and replication visibility are registered by
the host application. Geometry edits remain future work.

## Component and universe catalogs

`EntityStore` holds component blobs per world instance. Components use stable nonzero entity/type IDs, a schema version and a bounded payload. Startup validates registered component schemas before loading and restores entities, counters and initialization markers transactionally. Replication policy is explicit (`ServerOnly` by default); visible components are published through bounded per-client full/delta synchronization, and the client commits only complete validated transfers before acknowledging them.

`UniverseCatalog` is a bounded in-memory solar-system graph. It validates body IDs, parent ownership/cycles, finite values and body limits, then evaluates fixed positions or simplified circular orbits into observations. It is a catalog for renderer inputs, not a persisted world definition, a precise astronomy engine, or an N-body simulation. The client adapts observations from the existing astronomy `SkyState` path for sky rendering.

Planet movement derives local up from the radial direction, uses a tangent
movement frame and substeps collision checks. Dungeon movement keeps fixed Y-up.
Collision bounds use the corresponding capsule orientation. Source review of
the generated planet-to-dungeon route, portal return and v10 save/restore is
recorded in the E046 issue reports. Existing test and demo results predate this
worktree and do not verify it. Save migration is not provided; old formats are
rejected. Runtime restart/reconnect and rendered transfer remain unverified.
