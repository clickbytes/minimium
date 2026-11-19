# Excavation refusal: 2026-11-18 diagnosis and source correction

Base HEAD: `eb1aa15c27d0a81f7096e553101e43323464ffda`.
Working branch: `work/tool-viewmodels`, with the earlier local presentation
changes preserved. This report covers only the separate geometry correction.

## Observed evidence

The screenshot matches `logs/2026-11-18/000006/world.log`, server PID 7080,
parent client run `2026-11-18/000004`. Actions 2, 3, 4, 7 and 11 were refused
at 07:58:55–07:59:11 UTC, all with `OpenVoxelContour`, world instance 1.
Representative center: `[153553.6, 431582.0, 461419.6]` metres; selected chunk
`[9597, 26973, 28838]`. The other attempts report no surface within the existing
3 m tool reach. A geometry refusal proves the request reached server geometry
preflight; it is independent of the cosmetic held-tool model and animation.

Earlier runs also contain `TransitionSelfIntersection`. They are not evidence
that the new correction resolves every possible seam refusal.

## Confirmed source defect

`excavation_geometry::mesh_triangles` quantizes Marching Cubes fan vertices,
including the center, to millimetres. A thin fan triangle can become coplanar
with a chunk face. Previously `terrain_seam::collect_contours` combined its
coplanar perimeter with the section edges of neighboring triangles by set
union. Shared interior chords survived and could give the contour vertices
more than two neighbors, resulting in `OpenVoxelContour`.

Coplanar polygons also bypassed face-rectangle clipping: geometry elsewhere
on the same infinite plane could contaminate the requested face contour.

The runtime log identifies the failed contour class, but contains no failed
endpoints or incident triangles. The source failure mechanism is confirmed;
attribution of those exact five actions to that mechanism is not yet reproduced.

## Correction and ownership

- Omit only exactly zero-area millimetre triangles before both publication
  and contour preparation, using exact integer cross products.
- Count undirected edges across the complete selected voxel mesh. Count two
  is internal, count one is boundary, count above two rejects the candidate.
- Require every boundary edge to lie fully inside an exposed selected-chunk
  face. Internal holes cannot disappear by filtering away inconvenient edges.
- Build voxel loops from these actual boundary edges rather than plane
  intersections. Keep the degree-two/closed-loop and strip-intersection guards.
- Clip entire coplanar base polygons to face rectangles before collecting their
  perimeter, including the case where a triangle covers the whole rectangle.

Owner: `minimium-procgen` geometry integration. Server collision preflight,
client render preparation and teleport consume the same path. Base geography,
SDF samples, edit radius/material policy, generator digest/seed, wire v21 and
save v11 are unchanged. No world/profile file was modified or reset.

Failure remains fail-closed: `ToolPreparation::run` validates candidate geometry
before constructing the submitted world-change plan. Existing committed terrain
remains active; failed geometry does not reach journal submission/publication.
The new edge map is preparation-local with at most three entries per input
voxel triangle; no persistent state, I/O or worker is added.

## Changed files for this correction

- `crates/procgen/src/excavation_geometry.rs`
- `crates/procgen/src/lib.rs`
- `crates/procgen/src/terrain_clip.rs`
- `crates/procgen/src/terrain_seam.rs`
- `crates/procgen/src/voxel_boundary.rs` (new)
- `crates/procgen/README.md`
- `docs/plan/evidence/excavation-contours-2026-11-18.md`

## Verification

Independent read-only source review: PASS for precision, global edge incidence,
exposed-face ownership, validation ordering and full-polygon clipping. It does
not establish successful digging in the running game.

| Command | Result |
| --- | --- |
| `rtk proxy cargo check -p minimium-procgen -p minimium-server-runtime -p minimium-client --locked` | PASS / 0; initial iteration FAIL / 1 used PartialEq on MapSurfaceKind, corrected to matches! |
| `rtk proxy cargo check --workspace --locked` | PASS / 0 |
| `rtk proxy cargo check -p minimium-client --no-default-features --locked` | PASS / 0 |
| `rtk proxy cargo fmt --all -- --check` | PASS / 0 |
| `rtk proxy git diff --check` | PASS / 0 |
| `rtk proxy python scripts/check_architecture.py` | PASS / 0; 11 graphs |
| `rtk proxy python scripts/check_feature_contracts.py` | PASS / 0; all 22 consumer compilations, existing shared-example dead-code diagnostics only |
| `rtk proxy python scripts/check_file_lengths.py` | PASS / 0; 822 source files, max 500 lines, isolated combined-source overlay |
| `rtk proxy python scripts/check_static_refactor.py` | PASS / 0; isolated combined-source overlay |

The isolated tree `D:/Git/minimium/.local/worktrees/tool-viewmodels-check`
contains a SHA-256-verified overlay of all 29 locally changed files, including
the prior presentation work. This avoids archived nested-worktree declarations
being mistaken for duplicate current source. The geometry fix's seven paths
listed above are separate from the previous tool-viewmodel change.

Source review also retains an existing zipper limitation: coincident boundaries
with different vertex sampling are not explicitly short-circuited; this change
does not claim all `TransitionSelfIntersection` cases are fixed.

NOT RUN: test suites, geometry/runtime reproduction, game/GPU, benchmarks and
old-save execution under the final-testing gate. Read-only inspection of the
user's existing logs is the only runtime evidence here. The currently running
process cannot load source edits; both client and server need rebuilding and
restarting before evaluating the correction (the Earth launcher rebuilds both).

## Follow-up from the second screenshot

Server run `logs/2026-11-18/000009/world.log` records
`TransitionSelfIntersection` for every action from 2 through 16 in world 1.
Run `000012` records the same reason for actions 2 through 15. The server had
found a surface within reach and had already accepted the closed contours for
each of these attempts. The first screenshot's `OpenVoxelContour` occurred
earlier in geometry preparation. Action 1 in the second screenshot is a miss.

The old zipper advances around the two complete perimeters. Several short
voxel edges can consequently fan out from one distant base vertex and overlap
on a face. The original logs contain the brush center and touched chunks.
Future intersection diagnostics also retain the two rejected triangles in
world-space millimetres and the brush center to millimetre precision.

The new join fills the area between the two contours separately on each
matching chunk face. It splits both source meshes at common rounded crossing
points, checks the boundary of each fill, and orients the strip triangles to
their surface normals. Contours that follow different face sequences or run
along ambiguous box edges still use the spatial zipper and its intersection
checks. Preflight still precedes journal submission and world publication.

### Persistent cut material

An accepted dig is already stored and replicated as a change to the signed
distance field. `TerrainView::excavated_depth_m` compares its samples with
the corresponding base lattice before interpolation. A guard sample copied
from the base contributes exactly zero. The terrain renderer uses this
difference on the replacement mesh to blend the original cover into darker
earth or freshly exposed rock over 10 cm of removal. The coarse planet-space
texture coordinate is constant over each primitive, including during camera
movement. A refusal creates no persistent change.

### Follow-up files

| Owner | Files |
| --- | --- |
| Procgen | `Cargo.lock`, `crates/procgen/Cargo.toml`, `crates/procgen/README.md`, and `crates/procgen/src/{lib,sector_clip,terrain_contours,terrain_edge_splits,terrain_face_edges,terrain_face_strip,terrain_seam,terrain_zipper}.rs` |
| Terrain | `crates/terrain/README.md`, `crates/terrain/src/field.rs` |
| Renderer | `crates/renderer/README.md`, `crates/renderer/src/terrain_geometry.rs`, `crates/renderer/src/shaders/terrain.wgsl` |
| Server diagnostics | `crates/server-runtime/README.md`, `crates/server-runtime/src/runtime/excavation/ports.rs` |
| Project documentation | `docs/excavation.md`, `docs/plan/evidence/excavation-contours-2026-11-18.md` |

### Follow-up verification

Commands below used the working tree on the base HEAD above, with the `rtk
proxy` prefix. The isolated verification tree contained a SHA-256-checked copy
of the combined source changes.

| Command | Result |
| --- | --- |
| `cargo build --release --locked -p minimium-client -p minimium-server` | PASS / 0, both binaries |
| `cargo check --workspace --locked` | PASS / 0 |
| `cargo check -p minimium-client --no-default-features --locked` | PASS / 0 |
| `cargo fmt --all -- --check` | PASS / 0 on final sources |
| `python scripts/check_architecture.py` | PASS / 0, 11 dependency graphs |
| `python scripts/check_feature_contracts.py` | PASS / 0 |
| `cargo run --manifest-path scripts/wgsl_validator/Cargo.toml --locked` | PASS / 0, 25 shaders validated on CPU |
| `python scripts/check_file_lengths.py` | PASS / 0, 825 source files in the isolated tree |
| `python scripts/check_static_refactor.py` | PASS / 0 in the isolated tree |
| `git diff --check` | PASS / 0 |
| Digging at the reported coordinates, persistence after restart, second-player and camera-motion visual checks, automated tests | NOT RUN, project final verification phase |
