# minimium-terrain

## Purpose / Non-goals

Owns crater height fields, sparse volumetric terrain, immutable chunk images and preparation of bounded
edits in fixed world coordinates. The lattice is 0.5 m; chunks contain 32 cubed
cells. Generator sampling is supplied through `BaseTerrain`; this package has no
filesystem, network, GPU, worker or game-content dependency.

## Public API

`TerrainSnapshot` is the committed crater field, extracted from world. Its
existing render, collision and runtime consumers remain connected.
`TerrainStore::prepare` reads affected chunks and their unedited chunk guardband,
using a validated `TerrainDeformation` radius and allowed-material set without
knowing tool kinds. It returns resulting images with expected revisions. `commit` validates the
complete set before publication. Adapters persist every image before committing
them. Each sample belongs to one integer lattice address, including negative coordinates;
neighboring meshes read the same boundary samples. CPU Marching Cubes uses
canonical edge interpolation and an asymptotic decision on ambiguous faces.
Stores are cheaply cloned through shared immutable chunk images. `from_chunks`
reconstructs a committed snapshot for restore without invoking edit generation.
`TerrainDeformation::new` rejects invalid radii and material masks before the
store applies the existing configured radius and sparse-work limits.

## Dependencies

Depends only on world contracts. Base generation is an immutable `BaseTerrain` port.

## Data flow

The owner supplies a store and base field. Preparation returns resulting chunk
images and expected revisions. Mesh and spatial queries read the same field.

## Ownership and execution

Hosts own stores, scheduling, reservations, durability and publication. This
crate has no threads or external effects. It copies only touched chunk samples.

## Invariants and errors

Edits validate the finite deformation radius, allowed material set, every expected
chunk revision and capacity before publishing any chunk. The configured maximum
radius remains the upper work bound. IDs are committed in increasing order.
Negative coordinates use Euclidean division. `TerrainEditError` distinguishes
stale, unavailable and capacity errors.

## Commands

None; the runtime owns commands and authorization.

## Change guide

Change volumetric fields, preparation and meshing here. Neutral identifiers stay
in world; generation stays in procgen. Keep every source file below 500 lines.

## Verification

Compile with `cargo check -p minimium-terrain` and run architecture/file checks.
Tests and runtime acceptance remain deferred. `procgen` integrates Marching
Cubes chunks with clipped base surfaces and a fail-closed guarded contour strip;
it preflights affected sector geometry before the server publishes committed
chunk images and collision. The renderer builds a bounded base-mesh contour
prepass per selected LOD plus guard neighbors, and keys final patches by the
neighbor LOD/revisions used by those contours. Transition geometry vertices
shared with excavation surfaces are preserved during later edge conformance.
Runtime acceptance for brush-to-render and collision continuity across LOD is
still deferred.
