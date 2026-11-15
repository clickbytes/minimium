# minimium-fluids

## Purpose / Non-goals

Owns local water quantities, finite reservoirs and ocean exchange accounting.
The host provides cell capacities and permeable boundaries through an immutable
geometry port. This crate cannot modify terrain, perform I/O, launch workers or
use a GPU. A checkpoint contains both cell and reservoir quantities.

## Public API

`FluidSnapshot` contains cell volumes, capacities, MAC face velocity values and
reservoirs as one coherent value. `from_parts` reconstructs validated checkpoints.
`FluidSnapshotExt::step` uses bounded CFL substeps and conservative geometric
face flux with explicit geometry boundaries. `WaterRead` exposes fixed-sample
capsule observations to shared movement. `Reservoir` validates monotone curves;
ocean exchange contributes explicitly to `accounted_volume_m3`.
Each reservoir carries a low-order volume residual alongside its stored quantity.
Compensated additions retain exchanges smaller than the primary value's floating
point spacing. The residual follows finite volume or ocean exchanged volume;
the ocean's two terms are both negated in the total accounted water quantity.

## Dependencies

Only world contracts. `FluidGeometry` is a read port, not a terrain dependency.

## Data flow

Register prepared reservoirs, activate dry cells, then transfer existing water.
Capacity changes refuse to truncate occupied water. Snapshot values keep both
local water and reservoir balances together.

## Ownership and execution

The host owns authoritative state and checkpoints. No threads, I/O or GPU work.
The server must reserve world and server budgets before accepting an edit.

## Invariants and errors

Quantities and velocities must be finite, capacities are bounded by 0.125 m3,
and transfers fail before changing either endpoint. `FluidError` distinguishes
missing neighbors, capacity exhaustion and displacement requirements. Arithmetic
uses f64; no numerical correction silently creates or deletes water.

## Commands

None.

## Change guide

Keep water state and future solver algorithms here. Terrain supplies geometry;
runtime/storage own reservations, persistent encoding and publication.

## Verification

Use compilation and static architecture/file checks during implementation.
The CPU solver uses a 32-iteration Jacobi pressure projection and a Youngs-gradient
plane reconstruction. Plane volume and swept face occupancy use deterministic
midpoint quadrature, so this is a bounded geometric PLIC approximation rather
than an analytic clipped-prism implementation. CFL is capped at 0.5; step returns
capacity error instead of violating it when more than 4096 substeps are required.
Terrain capacities/apertures use sampled signed distance through
`TerrainFluidGeometry`; displacement preflights only connected active cells.
Runtime owns snapshots and supplies sampled `WaterSample` to shared movement.
Negative non-cell MAC boundary faces are persisted separately; `Air` is an
explicit zero-pressure endpoint that never receives water. Fluid surface planes
are exposed to renderer hosts for near-camera mesh tessellation. The renderer
uses partial-cell PLIC planes plus axis-aligned faces between full wet cells and
dry active neighbors, with the same bounded interface-cell selection for local
surfaces and subtracts those cells from catalog lake/ocean patches. Remaining
catalog surfaces are clipped to the current radial reservoir level and exact
base terrain.
Finite-reservoir active-cell exclusion is sampled at each level-curve point;
that rebase uses bounded 6^3 quadrature, not analytic basin recomputation.
Runtime-adjusted finite-reservoir curves may contain equal-volume plateaus,
including an all-zero curve when the remaining basin is fully represented by
active cells. Curve levels remain strictly ordered; inverse lookup selects the
upper end of a plateau, and an exhausted curve has zero capacity.
Runtime verification is deferred to the explicitly requested final test phase.
Each solver step snapshots each active cell's capacity and six faces once into
an immutable, per-step cache, reused by pressure iterations and CFL retries.
Cache entries are bounded by the snapshot's active-cell limit; unknown geometry
fails closed, and the cache is neither global nor shared across steps.
