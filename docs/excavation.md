# Persistent excavation and local water

This records the implementation and the remaining project-wide acceptance work.
Runtime and performance acceptance are deferred until the user explicitly opens
the final testing phase.

## World and controls

New worlds use generator 6, protocol 21 and save 11. The Earth launcher defaults
to `%LOCALAPPDATA%\Minimium\earth-v6`. Older saves are rejected without migration
or automatic reset. To explicitly start over, close the client and run
`.\start-earth.ps1 -RebuildWorld` (`-ResetWorld` remains an alias). This preserves
both selected world and profile directories as sibling `.backup-*` directories,
then creates a new Earth world and player profile. Active server/profile locks
prevent rebuilding a running session. Ordinary launches resume the saved world.

An authenticated developer can run `dev inventory tools` in the client console.
The grant is idempotent and the two-slot inventory belongs to PlayerProfileId.
The default bindings are `1` shovel, `2` pickaxe, `0` stow and left mouse
button to use the equipped tool. Stowing restores the existing fire action.
The confirmed equipped shovel/pickaxe has a closed, shaded first-person model;
it is hidden while stowed, loading, or viewing the console or overview map.
Bindings remain configurable. Console, map, focus loss and world transfer stop
continuous use. Tools have no drops, durability or stamina cost.
Both models share a 480 ms digging swing: wind-up, downward/forward strike,
short recoil and recovery. A queued local use starts the motion for click or
held-repeat; server refusal can still follow that attempted swing. Tool changes
and loss of control cancel playback. The shovel has a cupped steel blade and D
grip; the pickaxe has a forged eye and curved tapered points. Their viewmodel
depth is independent of the world and their projection follows window aspect.

Confirmed tool inventory also enables a developer HUD: current tool, first-hit
material/distance, a projected 0.6 m brush outline, and the last server result.
Green is a local material/range check, red is obstruction or wrong material, and
amber means no nearby target, synchronization or an outstanding action. The server
still validates geometry, water and persistence before accepting an edit. A geometry
join refusal is distinct from an object/player blocking the ray; the server records
the specific geometry error under `world.excavation_geometry_rejected`.

The server supplies position, equipment and the first collision hit. The sample
profile uses a 3 m reach, 0.6 m removal sphere, a 500 ms cooldown and one pending
action per player. Shovels remove soil; pickaxes also remove rock.

## Ownership and data flow

- `world` owns neutral chunk/revision/material, item, hit and water-read values.
- `terrain` owns sparse 32³ chunks at 0.5 m, signed fields, prepared edit images,
  Marching Cubes, independent crater fields and current-terrain queries.
- `procgen` supplies immutable ground, material layers, connected water catalogs
  and checked replacement geometry with guarded contour transitions.
- `fluids` owns bounded cell and reservoir volumes, MAC velocity/pressure,
  geometric VOF transport, capacity displacement and ocean exchange accounting.
- `simulation` owns authoritative tool admission, precise collision rays and
  the shared swimming/movement integrator.
- `server-runtime` coordinates reservations, existing workers, persistence,
  tick-boundary installation and bounded spatial replication.
- `storage` owns result-image transactions and coherent fluid checkpoints.
  `protocol` owns separately versioned bounded messages.
- `client-runtime` stages replicas and feeds prediction. Renderer consumes the
  completed state; it does not decide where water flows.

Geography does not carry mutable terrain. Explicit `TerrainInputs` separates
geography, a resolved manifest/map-bound material profile, crater fields, remnant
objects and volumetric chunks. The recognizing game-content resolver selects the
profile; runtime callers cannot override its soil depth. Tool rules continue to
enter through application composition.

## Structural transactions

A prepared edit contains expected chunk revisions and resulting images.
Preparation uses immutable views on the existing server worker pool. Collision
leaves and seam geometry are prepared before persistence. The owner rechecks
world context and input revisions, appends the result transaction, and installs
terrain and prepared collision only after a durable acknowledgement. Obsolete
worker tickets cannot replace the new geometry. Unchanged sector indexes are
shared through Arc.

`ExcavationCoordinator` privately owns preparation, durable pending work and
mutation IDs. Tool, impact, inventory and fluid-checkpoint paths share its
submission operations. Bounded work and publication ports do not expose the
whole runtime; live publication requires the owner's durable-acknowledgement
proof. Cancelling unsubmitted work drops its receiver and releases its exact
impact checkpoint reservations. Submitted writes are drained, not cancelled.

The coordinator serializes structural transactions with one bounded
pending preparation/commit; this is deliberately stricter than the per-chunk
limit. Independent water worlds can prepare in separate shared-pool tasks.
Only affected terrain images and world state are copied.

The `excavation.v2` journal persists chunks, inventories, fluid snapshots, independent
crater stamps, impact-body results and the catalog digest. Back up and restore it
with `world.snapshot` and the rest of the world directory. It rejects complete
corrupt records and truncates an incomplete final record. Checkpoints include
cells and reservoirs together. Compaction replaces a complete checkpoint before
retiring its covered journal prefix.

Water checkpoints are scheduled once per second; structural transactions include
the matching affected water state. Shutdown drains durable work and checkpoints
remaining dirty water. Crash recovery preserves confirmed state, not every wave.

## Replication and presentation

A peer receives nearby 3D chunks, then revisioned changes. Interest changes add
chunks to the retained replica, including deltas at the same world revision;
already received chunks continue receiving updates so distant LOD cannot restore
their original surface. Context changes and explicit resync start a full image.
The server retains full fluid state and reservoirs, but sends the client a
replacement image for a half-open 64-by-64-by-64 cell region around the player's
current terrain chunk.
The client discards cells outside that region, so moving interest cannot union
stale fluid cells into the active replica. Capsule water reads and movement
readiness are allowed only while the full capsule lies inside the installed
region; the client requests a new region as its anchor chunk changes. Reservoirs
remain a coherent full-world snapshot paired with the regional cells. Regional
updates are lazy and bounded by protocol part size; no performance benchmark is
claimed. Chunk and water bodies are split into protocol-sized parts; the server
generates parts lazily instead of queuing complete encoded images.
A structural batch publishes terrain, independent craters and its captured
fluid snapshot at BatchEnd. Context epochs invalidate old transfers and actions.
Missing revisions request resynchronization. Client movement waits for matching
collision and completed GPU terrain publication, including the volume revision.
Water interfaces are prepared on the presentation worker. Terrain, shoreline
and local-water buffers share the existing frame upload budget and are installed
as one completed selection. A water-only update retains terrain/shadow buffers
and does not invalidate the shadow publication. Pending water presentation can
retain the previous complete scene while the terrain revision remains current.
New wave snapshots coalesce behind active preparation, allowing completed water
frames to publish even when mesh preparation takes longer than a solver tick.

Geometry retains exact millimetre positions independently of the render origin.
Base terrain is clipped only in the replacement domain; checked contour strips
join it to the volumetric mesh. Surface metadata carries the cut material and
normal into rendering and collision. A failed preflight refuses the mutation.
Rendering constructs its transition contours from the actual selected LOD bases,
with edge conformance applied before joining and excavation seam points preserved
afterward. The extra preparation and cache are restricted to edited regions and
their neighboring coverage; unavailable or excessive coverage refuses preparation.
Successful digging exposes dark soil or freshly broken rock. The texture blends
with the original ground at the rim. Its coverage comes from the difference
between saved volume samples and the base field, so peers and returning players
can see the same cut. Unmodified parts of activated chunks retain their surface
cover. Excavated faces cast wall and ceiling shadows and carry bounded CPU
sky-visibility samples for local occlusion.

## Water model and bounds

The prepared catalog connects canonical cube-face samples, labels the largest
wet component as the fixed-level ocean and gives other bodies finite volume/level
curves. Catalog identity is checked against the saved digest. Ground collision
uses the actual bed rather than a sea-level floor.

Local cells use the terrain lattice. Water movement uses a nominal 10 Hz step,
radial gravity, pressure projection and geometric phase transport with CFL at
most 0.5. Geometry exposes solid, cell, air or reservoir boundaries explicitly.
Transfers debit and credit the same volume; finite reservoirs lose volume and
the ocean records signed exchange. Geometry changes preflight water displacement.
Reservoirs retain compensated volume residuals in memory, checkpoints and network
images, so small flows are not lost against a large lake's primary quantity.

The numerical implementation uses bounded iterative pressure projection and
sampled geometric interface integration. It does not claim an exact analytic
Navier–Stokes solution. The split and CFL requirement follow the
[Bridson notes](https://www.cs.ubc.ca/~rbridson/fluidsimulation/fluids_notes.pdf) and
[Basilisk VOF documentation](https://basilisk.fr/src/vof.h).

Defaults are 131,072 active cells per world and 524,288 per server. The startup
limits can be changed with `--fluid-world-cells` / `--fluid-server-cells`,
`server.fluid.world_cells` / `server.fluid.server_cells`, or
`MINIMIUM_SERVER_FLUID_WORLD_CELLS` / `MINIMIUM_SERVER_FLUID_SERVER_CELLS`.
The per-world limit accepts 1..262,144 cells and cannot exceed the server limit;
the server limit accepts 1..524,288 cells. Capacity refusals happen before
structural publication. Active water is retained when
players leave; memory eviction does not delete its volume. Oxygen, drowning,
erosion, rain, evaporation, air compression and surface tension are excluded.
World reservations include capacity for subsequent frontier growth and count
against the server cap. A refused solver step preserves the preceding complete
snapshot, emits a world-specific diagnostic and retries at a bounded interval;
other worlds keep stepping. Further structural edits in that world are refused
until its solver can proceed. Increasing the configured cap after restart is
the recovery path for exhausted capacity; no water is discarded to fit a limit.

Swimming uses the same collision integrator on both peers. Steering is relative
to the sampled current, buoyancy and drag precede collision, and vertical control
can overcome buoyancy. Developer flight remains a separate movement mode.

## Verification boundary

Compilation, formatting checks, architecture/ownership checks, the 500-line
limit and WGSL validation are implementation checks. Their latest result belongs
in the task handoff, not in a claim of gameplay acceptance.

The final phase must cover the user's complete two-player tunnel, finite lake
drainage and restart scenario, chunk seams/LOD, origin precision, duplicate and
concurrent actions, save failure and truncated journals, ocean connectivity,
swimming under ceilings, impacts near excavations and p95/p99 performance.
No unit, integration or end-to-end test suite is authorized before that phase.
