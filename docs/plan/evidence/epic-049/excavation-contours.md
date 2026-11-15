# Excavation contour follow-up

Issue: [#523](https://github.com/clickbytes/minimium/issues/523).
Investigated baseline: `3ad46a624010f240eae61db8d33220b8717b08a9`.
The PR/issue records the checked commit, exact commands, statuses and merge SHA.

## Observed refusal

The user's existing `logs/2026-11-15/000037/world.log` contains three
`world.excavation_geometry_rejected` events. Action 4 at
`2026-11-15T23:26:10.486Z` has reason `DegenerateTransition`, mutation 40,
world 1, center `(153564.4, 431603.2, 461399.7)` m and changed chunk
`[9597, 26975, 28837]`. Actions 2 and 3 have the same reason. These are
user-produced logs, not an agent-run reproduction. The screenshot's current
`No surface in reach` preview does not describe the earlier action's outcome.

`ToolPreparation::run` reaches `validate_volume_geometry` only after preparing
the candidate voxel edit (and candidate water if present). Geometry failure
returns through `ExcavationCoordinator::finish_preparation` as `UnsafeGeometry`.
The rejected candidate does not reach journal submission or active-world
publication. The client formats that authoritative rejection as the message in
the screenshot. Changing inventory, controls or deleting the save is not a
repair for this geometric failure.

The original error combined three branches: malformed triangle/face
intersection, invalid zipper input, and transition-strip intersection. A closed
loop already has at least three points before the zipper; a noncoplanar triangle
has at most two distinct intersections with a plane. Strip validation is thus
the relevant failure path under these input invariants. The old log does not
distinguish self-intersection from intersection between loop pairs.

## Static defect and correction

`terrain_contours::align` compared `a.iter().zip(rotated_b)`: with unequal
counts, it discarded the rest of the longer contour. The zipper then advanced
each side by `(vertex_index + 1) / vertex_count`. Neither operation measured
physical distance around the boundary. Subdividing one edge without changing
its shape changed the correspondence across the whole loop.

`terrain_zipper` now gives both operations one normalized cumulative Euclidean
length parameter, including the closing edge. Alignment retains deterministic
base anchoring and examines both voxel directions/all vertex rotations. It
integrates squared separation across the union of the two contours' breakpoints;
between breakpoints separation is linear, so the integral uses the endpoint
values exactly. Coordinates are sampled relative to a common integer origin
in f64; there is no planetary f32 projection in this calculation.

Zipper advancement uses those same breakpoints. It emits at most the sum of the
two point counts, keeps original integer boundary points and old/voxel surface
flags, and retains the existing minimum-area filter. Alignment still has bounded
work proportional to `voxel_points * (base_points + voxel_points)`, with one
candidate perimeter retained at a time. Invalid, zero-length or non-monotone
perimeters fail closed. No global cache, mutable owner, dependency or architecture
role changes are introduced.

Intersection predicates, contour pairing, closed-loop validation and final
sector clipping remain active. Distinct typed errors preserve invalid-input,
self-intersection and cross-strip intersection reasons in existing diagnostic
output. There is no fallback that approves an unsafe mesh.

This corrects the demonstrated sampling-density defect. Static analysis and the
old generic logs do not establish that it is the only cause at the user's saved
location, or that all possible crossing contours can be joined by a zipper.

## Consumers and compatibility

`replace_chunk_domain` uses the new helper for shared procgen geometry.
`validate_volume_geometry` preflights authoritative collision, while renderer
contour integration uses the same replacement path. No alternate executor or
unchanged consumer retains index-based pairing. Revisions, tickets, limits,
cancellation and durable-before-publication stay with the existing owners.
Protocol/save versions, generator/content digests, seeds and persisted voxel
bytes remain unchanged. Only the derived transition triangulation and local
diagnostic variants change; old saved chunks are interpreted through this same
geometry path.

## Verification boundary

Static commands and their exact checked SHA are attached to #523 and its PR.
Game, CPU reproduction workloads, unit/integration/E2E suites, GPU sessions and
benchmarks are NOT RUN under the user-controlled Q03 gate. No test suite is added.

Deferred scenarios: repeat both tools at the reported Earth coordinates;
nonuniform/sparse/dense loops, reversed winding and rotated starts; coincident
boundaries and millimetre slivers; multiple chunks and loop pairs; explicit
self/cross intersections and open/invalid contours; matching movement/tool/render
geometry after durable publication and no world change after rejection. Record
the now-distinct diagnostic reason if the original location still refuses.
