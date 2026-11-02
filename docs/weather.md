# Regional weather and evolving voxel clouds

The client reconstructs local weather from the manifest seed, planet radius and
world UTC. `AnnualMean` is the only seasonal profile; calendar dates do not switch
weather seasons. Astronomy keeps its independent seasonal lighting. Network
messages and saved-world formats are unchanged.

## Geography and altitude

`environment/weather` has no camera or GPU dependencies. `WeatherModel::sample`,
`WeatherModel::query` and immutable `CloudSnapshot` publications remain its
interfaces. Conditions use latitude, terrain, local solar time, upwind ocean and
lake samples, orographic uplift and slowly advected humidity noise. Shared lake
polygons live in `environment/lakes`.

Regions address a 128 x 128 cube-sphere lattice. Conditions are continuous across
region, longitude and face boundaries. The player anchors the local area even
while viewing the globe. Coverage fades outside 0.034 radians and ends at 0.055;
the source search includes lifetime wind displacement and full shape bounds.
The remainder of the planet has no clouds in this stage.

Reference lengths and velocities are scaled by `manifest radius / 6,371,000 m`.
The following are model envelopes, not a complete meteorological simulation:

| Source floor | Reference base | Families | Reference upper bound |
|---|---|---|---|
| Low | 500-2,000 m above local terrain | Stratus, Stratocumulus, Cumulus | Local terrain + 3,500 m |
| Low, spanning floors | Same low base | Nimbostratus | 7,000 m above reference sea level |
| Low, convective | Same low base | Cumulus congestus | Base + 6,500 m, capped at 10,000 m |
| Low, convective | Same low base | Cumulonimbus | 12,000 m |
| Middle | 2,000-6,000 m above reference sea level | Altocumulus, Altostratus, lenticular forms | 7,000 m |
| High | 6,000-11,000 m above reference sea level | Cirrus, Cirrocumulus, Cirrostratus | 12,000 m |

Actual bases are driven by humidity, temperature and terrain clearance within
these envelopes. Thickness, horizontal size, aspect ratio and optical depth vary
by family. Warm, moist uplift favours convective towers; cooler, moist conditions
favour low sheets; uplift enables middle-level lenses. High floors select thin
filaments, cellular patches or veils. The 12 visual forms include all ten cloud
genera plus congestus and lenticular morphology variants. Cumulonimbus and
nimbostratus describe appearance only: this does not implement storms or rain.

The conservative swept terrain bound includes every morph position. GEBCO uses
cached one-degree maxima (259,200 bytes), including interpolation and terrain
detail. Procedural planets use their declared relief bound. If safe clearance
would push a family outside its allowed altitude, that candidate is suppressed;
low stratus is not silently relocated to the high floor. These conservative bounds
can suppress low clouds near mountains, particularly on procedural planets.

## Shape and deterministic evolution

IDs include the seed, source region, source floor, temporal population and slot.
Populations begin every 1,800 world seconds with independent birth offsets;
clouds live for 3,600 seconds. Coverage controls occurrence and optical depth.
Birth conditions select the family; later populations follow changing weather.

`families.rs` defines profiles and altitude limits. `shape.rs` defines a continuous
field in cloud-local metres, reconstructed from the cloud ID and age in f64:

- Cumulus forms use a binary hierarchy with four recursive subdivisions (31
  lobes), inherited motion, asymmetric children and a flattened lower profile.
- Congestus stretches that hierarchy vertically; cumulonimbus adds an anvil.
- Stratocumulus, altocumulus and cirrocumulus use overlapping cellular kernels
  at different scales. Neighbour evaluation avoids address-boundary jumps.
- Stratus, altostratus, nimbostratus and cirrostratus use sheets with different
  thicknesses, opacity and eroded edges.
- Lenticular forms use stacked lenses; cirrus uses curved, tapered filaments.

Bounded spatial deformations, independently phased lobe motion and advected noise
change the occupied cells over time. They are a procedural approximation, not a
fluid solver. Recursive shape parameters are independent of voxel resolution.
No LOD transition re-seeds the field or changes the cloud ID.

The entire cloud also advects eastward at reference 5/10/20 m/s according to its
source floor. The polar speed tapers when cosine latitude is below 0.03. Absolute
UTC and spherical transforms use f64 before camera-relative GPU conversion;
neither motion nor morphing integrates frame deltas. Pausing UTC freezes both.

## Recursive resolution and sparse voxels

N is the dyadic subdivision depth of a cloud's bounding cube: the reference cell
size is `longest cloud side / 2^N`. Cropped cloud bounds align to the lattice.
The field supports N up to 6 (at most 64 cells along any axis). A family-specific
minimum preserves thin layers; ordinary profiles retain at least two cells in
their narrowest dimension. The fixed N=6 safety cap can limit this near ceilings.

The worker selects detail by player distance and bounded work, using the same
selection in FPP and globe views. `weather clouds detail 2|3|4|5|6` changes the
preferred maximum, default 6. The thin-layer minimum takes precedence. Each
cloud has one resolution at a time; this stage does not introduce mixed-level
octree leaves inside a cloud. Sparse 8 x 8 x 8 bricks store only occupied regions.

Each cell stores two quantized coverage samples. Per-cloud windows are 12 seconds
for convective forms, 18 for high filaments/cells, and 24 for other forms, with
independent offsets. The GPU interpolates coverage from absolute cloud age every
frame. Faces store signed coverage differences between neighbours at both ends
of the window, so a face can be revealed as its neighbour evaporates. Equal
interior faces are removed; compatible external faces are greedily merged.

A bounded cache reuses meshes for unchanged cloud windows and resolution/face
budgets. Face budgets are quantized to avoid invalidating all cached clouds when
one candidate appears. Only expired or changed entries are rebuilt; this version
caches whole cloud windows rather than maintaining incremental dirty-brick
meshes across windows. If a publication is late, the previous morphology clamps
at its endpoint while its spherical position continues to follow UTC.

## Rendering and budgets

Visible geometry consists of cube faces, with no ellipsoid renderer, ray marching
or cloud-interior fog. Sun, Moon and sky ambient light illuminate the surfaces.
An independent cloud projection, scene-depth sampling and analytical planet
occlusion work in FPP and globe views. Clouds composite after clear-air atmosphere.

Optical depth depends on family, coverage and physical profile, never directly
on N or the number of cells. It is a stylized surface approximation: exposed
density differences are not a full volumetric light integral. Weighted blended
OIT uses RGBA16Float accumulation and R16Float revealage; unsupported devices
report `sorted_face_alpha` and sort faces in the worker.

Hard limits remain 8,192 cloud records, 16,384 cached source records and 131,072
48-byte faces per publication. GPU buffers use approximately 6.63 MiB (80-byte
cloud records and a 256-byte uniform). Three mesh slots total at most 18 MiB.
The temporal cache stores at most another 6 MiB of faces plus bounded metadata;
one sparse voxel scratch volume is at most 512 KiB of cell pairs plus its index.
The conservative CPU working envelope is 64 MiB, excluding shared geography.
OIT targets retain their separate 64 MiB cap and downsample above that limit.

At most 8,000,000 density evaluations are requested per build. Face budgets favour
nearby clouds and reserve six faces per record. Budget degradation aggregates
dyadic blocks of the same sampled field without growing its physical bounds.
The final emergency representation is its bounding volume with sampled mean
coverage; it loses silhouette detail and is reported as simplified. Thus a hard
budget can visibly reduce shape diversity, but cannot generate replacement IDs.

One persistent worker has one replaceable pending request and one completed slot.
It publishes complete snapshots at requests spaced at least 500 ms apart. World,
manifest, clock revision and requested detail must match. Obsolete world/detail
work is cancelled between clouds. Newer requests do not partially overwrite the
visible snapshot. All publication and GPU buffers remain bounded.

## Diagnostics

- `weather status`: conditions, source floor, wind, family counts and actual
  altitude bounds in world metres, effective N histogram, sparse brick count,
  density evaluations, cache hits/bytes, simplification count, publication lag
  and stalled morph windows. N=0 denotes emergency bounding geometry.
- `weather clouds detail 2|3|4|5|6`: preferred maximum N; retains thin-layer minimum.
- `weather clouds debug off|regions|layers|wind|kinds|detail`: region boundaries,
  source floor colours, eastward arrows, family colours or effective detail.

Layer colours identify the source floor: a low-origin convective tower can extend
above it. The detailed family altitude bounds in `weather status` show that span.

## Static verification and later functional review

Implementation checks use RTK: formatting, workspace compilation, Clippy, Naga
WGSL validation including CPU/GPU layout contracts, architecture, file lengths,
`git diff --check` and a release build. No tests or game launches are part of this
phase. Release output uses `target/weather-release` to avoid executable locks in
the ordinary target directory.

After interactive verification is authorized:

1. Compare ocean, dry land, moist land and mountains. Inspect low, middle and high
   family selection with `debug kinds` and altitude bounds in `weather status`.
2. Watch one stationary terrain landmark and several cloud edges: translation,
   local growth, erosion and branching should be visible independently of fades.
3. Pause UTC, switch FPP/globe and change detail between 2 and 6. IDs, family,
   placement and large structures should remain consistent. Inspect thin cirrus.
4. Observe temporal-window boundaries and neighbour faces while clouds reshape.
   Check for holes, flicker, opacity jumps and synchronized whole-sky pulsation.
5. Cross regions and cube faces, the date line and poles. Incoming clouds must
   keep their identity, morph phase and terrain clearance.
6. Teleport, revise/accelerate/reverse the clock and switch worlds while work is
   pending. Check stale-result rejection and `morph_stalled` under heavy load.
7. Review face-budget aggregation and N changes: silhouettes and transparency
   can vary under simplification and require visual assessment on real hardware.
8. Inspect overlapping clouds, terrain silhouettes and the far hemisphere in
   daylight, moonlight and night; resize windows and inspect memory limits.
9. Review sorted-alpha fallback on unsupported OIT hardware, including rapid
   camera rotation, and inspect low-resolution OIT edges.

Appearance, smoothness and actual runtime performance require that later review.
Precipitation, wet terrain, storms and cloud shadows remain outside this stage.

## Design references

The implementation is original Rust/WGSL. Its shape hierarchy is informed by
[Bouthors and Neyret, Modeling Clouds Shape](https://evasion.inrialpes.fr/Publications/2004/BN04/);
family-specific procedural fields by
[Procedural Cloudscapes](https://perso.liris.cnrs.fr/egalin/Articles/2018-cloudscapes.pdf);
multiscale coherent deformation by
[Perlin and Neyret, Flow Noise](https://evasion.inrialpes.fr/Publications/2001/PN01/).
Sparse blocks are inspired by the storage principles in
[Museth, VDB](https://museth.org/Ken/Publications_files/Museth_TOG13.pdf), without
adding an OpenVDB dependency. No cited renderer or fluid solver is imported.
