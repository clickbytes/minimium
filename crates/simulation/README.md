# minimium-simulation

## Purpose / Non-goals

Owns mutable player state, movement integration, collision indexing and queries, combat, world snapshots and fixed-step results. It does not perform I/O, generation, protocol encoding or GPU work.

## Public API

`World` owns mutable simulation state; `MovementIntent`, `MovementPolicy` and `step_character` provide the shared server/prediction movement path. Intents contain direction, jump and vertical control only. The server owns movement limits and flight policy, which are copied into worker batches before parallel stepping. `step_batch` processes immutable batches, while `CollisionQueryStats` reports bounded query work.

`SpatialGeometryPublication` atomically owns the movement collision index, the
full-map combat baseline and exact-sector replacement indexes under one revision.
Hitscan uses the shared `CharacterGeometry` eye point and transforms the ray into
each target's local frame before intersecting the shared body box. Its broadphase
still covers neighboring cells for rotated bodies. Hitscan requires a
`CombatCoveragePlan` certified for the geometry revision, origin, direction and
range. Filtered baseline queries continue after masked hits and test box
entry/exit points, so replacement sectors suppress stale blockers while unchanged
portions of a box remain solid. A missing replacement returns typed
`ShotReadinessError::NotReady` before player health changes.

The optional neutral `WaterRead` view feeds the same `step_character_with_water`
and precise-millimetre integrators used by prediction and authority. It applies
buoyancy, flow drag and bounded vertical swim control before collision movement;
legacy entry points remain dry. Immersion does not model oxygen or drowning.
Steering uses velocity relative to the local current; vertical swim acceleration
can overcome buoyancy in either direction. Water forces are applied after
steering/gravity and before collision, independently of developer flight.
Radial-gravity movement sweeps the oriented capsule against solid terrain
triangles with bounded substeps, so excavation walls and ceilings block motion
while the projected movement slides along contacted walls.

## Dependencies

Consumes world maps, geometry and manifests from `minimium-world`, plus core player identifiers. The world domain does not depend on simulation.

## Data flow

Runtime supplies input and an explicit delta; simulation returns state/events or batch results. Workers read immutable collision data, and the owning runtime applies results in order.

## Ownership and execution

Server runtime owns authoritative `World` instances. Client runtime owns predictive state. Both call the same movement integrator; workers never mutate active worlds.

## Invariants and errors

Preserve fixed-step duration, movement range, gravity/collision algorithms, deterministic batch merge order and combat rules. Runtime publications retain full-map baseline coverage outside installed replacement sectors; server coverage plans request the complete conservative ray-sector set before a shot can use those replacements.

## Commands

Movement and combat declarations belong to this domain; host adapters own dispatch.

## Change guide

Change movement, collision or combat rules here. Keep geometry definitions and topology addressing in `minimium-world`.

## Verification

Run `rtk cargo check -p minimium-simulation -p minimium-server -p minimium-client -p minimium-protocol` and architecture/file-length checks. Runtime acceptance is deferred.

## Authoritative impact motion

`impact::step_body` is a pure SI gravity/drag/ablation integrator using a borrowed
`BodyEnvironment` surface port. Swept sphere contact uses bounded substeps and
bisection; one pressure-triggered daughter preserves momentum. The runtime
supplies fragment capacity, so saturation does not silently delete fragment mass.
This is a bounded gameplay approximation, not a hydrodynamic impact solver.
`CollisionIndex::from_partitions` retains immutable per-sector indexes and cached
bounds. Character and ray queries visit nearby partitions; edits rebuild only
changed leaves and a lightweight root. Static map collision remains connected.

`tools::ToolUseGate` prepares bounded tool actions from server-owned inventory,
application-supplied `ToolDefinition` values and a first-obstacle scene port.
`ToolProfile` requires one definition for each current `ToolKind` and keeps one
shared cooldown. The gate selects a definition from confirmed inventory, checks
its allowed-material set and reach, and allows only one pending action, with
monotonically ordered IDs and an explicit completion. The server runtime supplies
current collision hits and the profile, then finishes the gate after a durable
mutation or a typed refusal. The resulting definition becomes a validated generic
terrain deformation; terrain does not branch on tool kind. The same exact
triangle and player-capsule query blocks digging behind an earlier obstacle.
`CollisionIndex::tool_surface_hit` exposes that read-only nearest-hit query to
client previews. Callers pass nearby remote-player `ToolRayCapsule` blockers
containing the same `CharacterGeometry`; the authoritative
`World::tool_surface_hit` delegates through the same query. Camera, hitscan and
tool rays share the 1.6 m eye point.
