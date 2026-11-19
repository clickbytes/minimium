# minimium-procgen

## Purpose / Non-goals

Owns deterministic, addressable world generation, planet and dungeon geometry, and world materialization. It validates generator compatibility for manifests before materialization. Earth sampling uses the `PreparedGeography` port; this crate does not read files or own a global package. Worker scheduling, transport and rendering are outside this crate.

## Presentation surface

SurfaceRecipe supplies validated, presentation-only landform parameters.
TerrainSurfaceSample includes physical source elevation, retention, exposure and
coverage provenance. Landforms use the same fixed height neighborhood across
sectors/LODs/faces. Sampling does not change generated mesh, collision or generator
identity. Hosts select the recipe through PlanetTerrain::with_surface_recipe;
sample defaults are owned by games/sample.

## Public API

`terrain_surface::TerrainSurfaceSample` contains elevation, slope, water,
world-space normal and six ordered material fractions. Sampling uses a fixed
physical derivative footprint across sector/face/LOD boundaries. The optional
`PreparedGeography::land_cover` port supplies interpolated real-world fractions;
otherwise procedural fractions apply. This presentation path leaves `MapTriangle`,
generation digests, saved geometry and collision sampling unchanged.

Static planet triangles and box faces are clipped to each cube-face cone before
sector UV bounds are calculated. Cross-seam triangles and boxes spanning many
sectors are retained in every touched payload; boxes containing the origin are
included in every sector. Streamed replacements therefore preserve blockers
from the full-map geometry throughout each ready sector domain.

Without WorldCover, presentation fractions distinguish arid latitude belts,
temperate ground and alpine rock using physical source elevation. They no longer
assign the same grass-dominated mixture to all low-elevation land.

See the re-exports in `src/lib.rs`, including sector materialization, planet sampling, dungeon generation, generator contract validation and addressable randomness. `PreparedGeography` is the host port for immutable geographic samples. `PreparedTerrainSource` privately binds one world instance, manifest and exact map fingerprint to its validated material profile, optional relief policy and prepared geography. Its constructor checks material/relief bindings and requires matching prepared geography for Earth. It has no `Default` or setters. `TerrainInputs` combines that source with a separate `TerrainOverlaySnapshot`; both are required by manifest terrain and sector consumers. The overlay snapshot is instance-bound and preserves the independent edit, remnant and volume revisions. `PlanetGenerationSource` is a separate explicit recipe for pre-map generation, where a final map fingerprint cannot yet exist; it is not accepted by runtime geometry consumers. Sample world composition belongs to `minimium-game-sample`.

Generated-planet relief sampling uses a neutral `PlanetReliefSamplingPolicy` supplied by the composing game and a `ResolvedPlanetReliefPolicy` bound to the manifest and map fingerprint. Procedural generated planets require this binding in `for_manifest`; Earth uses identity policy and retains its world-owned `PeakRelief` curve. Procgen evaluates the recipe but owns no scenario face, route, spawn, marker or fallback policy. These content bindings are runtime composition values and add no manifest, save or generator-digest fields.

Excavation collision preflight and render contour preparation share projected
chunk bounds. Neighbor queries match canonical cube corners across face seams.

## Dependencies

Depends on `minimium-core` and `minimium-world` plus serialization, hashing and memory mapping libraries; world supplies manifests and neutral dungeon layout types. It has no dependency on sample-game content.

## Data flow

World and sector inputs produce generated maps, sector geometry and sampled terrain. Hosts open geography through `minimium-asset-io`, prepare a `PreparedTerrainSource` after map/profile/policy resolution, then pair it with overlays for workers. Pre-map generation uses `PlanetGenerationSource`. Missing or mismatched Earth geography returns a typed sampling error; client transition work keeps that error attached to its content-resolution ticket.

## Ownership and execution

Generation functions are called by client/server owners or their scheduled work. Hosts own prepared geography and attach it through the validated source before Earth sampling. Workers receive immutable source/overlay snapshots; caller-owned context/ticket checks still gate result installation and publication.

## Invariants and errors

Preserve seeds, address calculations, rounding, scales and algorithm order. Generation must remain reproducible for the same inputs. Manifest validation and map materialization return `GenerationError` with a stable code and concrete category rather than requiring callers to parse an error string.

Terrain, source elevation, bounds, meshes and water catalog sampling
return `GenerationError`; `Sampling(EarthSampleError)` preserves a failed provider
query, including nonfinite elevation. No failed sample becomes an empty mesh or
invented height. `ExcavationGeometryError::Generation` carries the cause through
sector preparation. Existing optional spatial-query and `BaseTerrain` ports map
failure to unavailable (`None`), so callers cannot approve an unsupported hit.
Callers publish only a complete successful result and still validate its context
and revision. Successful sampling arithmetic and content digests are unchanged.

Render patches and coarse blocks sample signed ground heights through
PlanetTerrain::ground_sample; Earth bathymetry remains below sea level.
PlanetTerrain::sea_position supplies canonical sea-level vertices for presentation.
The renderer owns the separate ocean mesh and its intersection with ground.
The earlier binary shoreline clipping is removed. PlanetTerrain::sample and
collision sector generation retain their current ground results and rounding.

## Commands

None.

## Change guide

Change terrain or dungeon algorithms here. Keep file discovery and persistent data ownership outside this crate.

## Verification

Run `rtk cargo check -p minimium-procgen` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.

Generator revision 6 preserves PlanetDefinition::earth_elevation_mm for ground
while separating the water surface from collision and introducing the immutable
0.5 m excavation lattice. The v6 digest is the accepted generated-world contract.
World kind selects the
procedural planet, Earth, or dungeon algorithm explicitly.
Source geography remains available for climate independently of shaped terrain.
Height derivatives and both planet views follow the same mapped field.

`WaterCatalog::reference_sea_radius_m` exposes the baseline water radius for
scaling conformed render facets to a reservoir level.
`WaterCatalog` samples a bounded canonical cube-face grid and flood-fills wet
components across shared face-edge sample keys. Its largest connected component
uses a stable ocean ID; finite components publish sampled level-volume curves
and a digest. Curves and shoreline assignment are a bounded terrain catalogue
approximation, not a high-resolution hydrology solver.

PLANET_RENDER_BLOCK_SUBDIVISIONS publishes the unchanged 16-cell presentation
lattice to renderer coast probes, projected-cell scoring and edge conformance.
Collision subdivisions remain independent; generation output is unchanged.

Generated manifests require revision 6, a topology-matching generator digest,
and topology-specific planet settings. Authored manifests use an `AUTH`
fingerprint marker; hosting and client composition verify it against the
materialized map before publishing or installing that content.

## Committed terrain overlays

`PreparedTerrainSource` carries geography and resolved material/relief policy
bound to one instance, manifest and map fingerprint. `TerrainInputs` cannot be
default-constructed and combines it with `TerrainOverlaySnapshot`, whose world
instance is checked against the source (and whose volume store has its own world
identity checked). `volume_base` and volume materialization can therefore only
consume a resolved profile. The binding lives in runtime composition/session
state and adds no world-manifest, protocol or save fields. `SectorContentVersion` carries separate
monotone revisions for edits, remnants and volume mutations. Sector cache
invalidation uses this complete global version, so an unrelated source mutation
may rebuild more sectors but simultaneous changes cannot cancel each other.
`PreparedGeography` no longer exposes terrain state.
Height, normals, exposed rock/soil coverage, collision, render patches, teleport
ground queries and remnant placement sample that same overlay. The base geography
is immutable. `SectorGeometry::object_triangles` keeps
the collidable seeded meteorite mesh separate from terrain material sampling;
both geometry consumers include it. Remnants have independent spatial/revision
queries. `PlanetVolumeBase` supplies generated ground and material classification
from the bound profile to `minimium-terrain`; consumers cannot override its soil
depth per call. `validate_volume_geometry` preflights affected collision
sectors: it clips old triangles from activated chunk boxes, extracts canonical
box-face contours, zippers matching Marching Cubes loops and returns an error for
open, unmatched or degenerate transitions. Voxel contours use
the actual exposed edges of the quantized mesh, rather
than intersecting all fan triangles with infinite chunk-face planes. Exactly
zero-area triangles collapsed by millimetre rounding are omitted before both
publication and edge-incidence counting. Two-triangle edges are internal;
one-triangle edges must be fully contained on an exposed selected-chunk face.
Incidence above two, interior holes and non-closing/branching contours still
reject the candidate. Base-mesh coplanar polygons are clipped to the complete
face rectangle before perimeter extraction, including full-rectangle coverage.
The incidence map has at most three entries per input voxel triangle and is
bootstrap/preparation-local, with no extra persistent owner or worker. The
replacement is shared by collision preflight, render patches and teleport;
failed candidates still cannot reach persistence/publication. Render workers use
`integrate_volume_sector_with_contours` with their LOD mesh and neighboring face
triangles. Transition strips are intersection-checked against other loop pairs
before they can be returned for publication. `terrain_clip::clip_triangle_outside_boxes`
is the shared exact triangle/voxel-box subtraction hook for renderer sea meshes.
Collision, render invalidation and contour preparation share the same chunk-key
selector. Contour source sectors come from exact cube-face UV bounds of each
selected chunk AABB (with positive-denominator extrema at its corners), expanded
by one sector; center-crossing or over-limit footprints fail closed. Render
contours use selected LOD bases where available and bounded guard bases elsewhere.
Teleport surface queries intersect the integrated mesh and choose the outermost
exposed hit; the runtime applies its player clearance above that point. GPU
runtime acceptance is pending.

When both contours traverse the same exposed chunk faces, preparation pairs
their runs at box edges and triangulates the area between the two paths on
each face using the integer even-odd fill in `i_triangle`. Coincident paths
cancel, including differently subdivided straight edges. Crossing paths use
one canonical millimetre vertex on both source edges and the transition.
Only affected source triangles are subdivided. A longer clipped base edge
receives both ends of the face span and all its crossings. An unpaired
intermediate clipped vertex rejects the candidate. Winding follows the
outward source normals and vertex metadata uses the same interpolation as
sector clipping. A guard triangle from another sector
can supply a contour without being part of the current sector's output.
Preparation checks each input path for self-intersection, certifies the
triangulation's boundary against both input paths and rejects a missing split
or inverted source triangle. All geometry is local to the current candidate.

Loops with different face sequences or spans along ambiguous box edges use
the checked spatial zipper. Its alignment and advancement use cumulative
Euclidean arc length normalized over the complete loop, with both directions
and all voxel vertex rotations scored by integrated squared separation.
Both methods run the strip intersection checks. `TransitionSelfIntersection`
and `TransitionStripIntersection` distinguish these refusals in existing server
diagnostics; `InvalidTransitionContour` rejects invalid perimeter input, and
`DegenerateTransition` remains the malformed face-intersection error. These
diagnostic variants do not change the protocol's `UnsafeGeometry` rejection.
No failed candidate is returned to the runtime for persistence/publication.
