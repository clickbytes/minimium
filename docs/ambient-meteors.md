# Ambient cosmetic meteors

These are local presentation events, separate from the server bolid demo.
Two players are not guaranteed to see the same meteor. Neither this scheduler
nor its events enter world simulation, networking, saves or generation.

## Scheduler (E047-04)

The client captures one session seed at runtime creation (wall-clock nanoseconds
mixed with process identity). `MINIMIUM_METEOR_SEED=<decimal u64>` overrides it
for debugging; the world membership context is mixed into the seed when that
scope becomes valid. Same seed, context, monotonic epoch and observer inputs
reproduce the candidate sequence within this build. The context hash is not a
persisted or cross-version contract. No per-frame entropy is consumed.

Candidate n occurs at `epoch + 120 + 210*n + jitter(n)` real seconds, where
seeded jitter is in `[0,90)`. Thus successive intervals are within 120..300
seconds. A counter-based hash chooses 94.9% faint, 5% bright, 0.1% fireball.
These are presentation choices, not measured astronomical occurrence rates.
The mean rate is approximately 17.14 candidates per real hour while enabled.

Every update computes the latest due slot in constant time. At most one
candidate is considered per frame; older slots count as skipped and an already
expired latest candidate is skipped too. Accepted events retain their scheduled
birth time, so frame rate does not change their age. No backlog is allocated.
UTC changes, pause and time scaling do not affect this monotonic schedule.
Unavailable environments, interiors and world-scope changes invalidate events
and reset the schedule; re-entry starts a new bounded delay. Manual spawn/clear
does not consume, shift or reseed the automatic queue.

Automatic placement uses observer position and seeded tangent directions,
independently of camera heading, FOV, daylight or cloud coverage. Rendering
determines visibility separately. The manual diagnostic placement still uses
camera heading. Track starts are 80..110 physical km; the seeded entry basis is advanced by the physical model described below.

`MINIMIUM_METEORS=0` disables automatic births at startup; the enabled flag is
runtime-local and will join the typed property registry in E047-14. Manual
commands remain available. `sky meteors status` reports enabled state, seed,
interval, next deadline, spawned/skipped totals, active peak and current gate.
`sky info` reports actual rendering counts and the altitude gate separately.

Active events share the eight-event cap, preallocated once. Each has eight trail
slots; the frame separately caps 16 cores, 128 trail points and four lights.
Sources outside the view still expire. No fragment or dust allocation is added
by this stage. Numerical compilation/static verification is separate from the
project's gated runtime and visual acceptance.
## Ablation and photometry (E047-05)

The CPU renderer model supplies six seeded presentation materials: stony, iron,
brittle, porous, heterogeneous and cracked. Densities span 800..7800 kg/m³;
effective ablation heat is 3..8 MJ/kg, heat transfer 0.04..0.16, erosion
multiplier 0..1.2 and visible efficiency 0.015..0.035. These are approximate
profiles, not measured composition inference. Material strength is reserved
for the next fragmentation stage. Preset initial masses are 0.002, 0.5 and
100 kg; seeded entry speeds remain within 11..72 km/s.

The solver uses f64 SI coordinates, exponential atmosphere density with a
7200 m scale height, sphere cross section from current mass/density,
drag-proportional heating and bounded mass loss. Exponential velocity damping
cannot reverse the body. Visible power follows the loss of kinetic energy with
a 60 ms gas cooling response; physical radius falls with the cube root of mass,
while luminous radius follows solid radius and visible power separately.
There is no combustion flame simulation, forced explosion or impact entity.
Gravity, full atmospheric profiles, spectral lines and CFD are outside this
phenomenological model. One of four fixed palettes varies gently with speed;
continuous seeded flicker does not reroll colour each frame.

`Evolution` precomputes at most 512 fixed samples, four 10 ms substeps per
40 ms sample, once per event. Frames interpolate that history, including past
trail positions. Integration stops after optical cooling of exhausted/slowed
material or atmospheric exit, with a 20-second hard presentation cap. A lower
35 km cutoff prevents cosmetic impacts; residual solid mass can survive after
emission and is discarded only as local presentation state. Runtime diagnostics
report material, remaining mass, speed, solid radius and gas emission power.

The same emission supplies source intensity (`power / 4π`) and, in the GPU adapter, visible-core
radiance (`intensity / projected luminous area`). Angular filtering supplies
inverse-square dilution for unresolved sources, without an additional distance
division in the shader. The current wake receives 15% of source emission spread
over its total projected geometry area; it does not multiply source power by
trail length. A deposition-driven persistent train is the next trail stage.
HDR exposure and output transfer remain shared. Absolute radiometry and visual
acceptance are not claimed; units here establish a consistent local model.


## Fragmentation and erosion (E047-06)

`fragmentation::System` owns up to four solid bodies and four grain packets per
event. Dynamic pressure `q = 0.5*rho*v�` crossing the seeded material strength
splits a surviving body; height alone never triggers breakup. At most two
breakup generations and three daughter slots bound the cascade. Raised
post-failure thresholds prevent repeated triggering at the same pressure.
The faint 0.002 kg profile is below the 0.02 kg breakup eligibility floor.

Each split transfers 20..45% of the remaining parent mass to a daughter at the
same release position, with seeded 5..25 m/s transverse dispersion. A tiny
forward-speed reduction pays the dispersion kinetic-energy budget. Gas power
is shared by the same mass fraction, so inherited glow is not duplicated.
Each body subsequently has independent drag, ablation, phase, position, speed
and emission; increased surface area can create one or several energy-driven
flares, including late failure. Strong material can ablate without breakup or
an energetic flare. There is no imposed explosion pulse or physical implosion.

Ablation and eroded dust are separate mass flows. Eroded mass enters a bounded
accumulator and may transfer once into a grain packet. Packets represent many
fine grains using an effective area profile; the packet counter is not a
meteoroid count or extra mass. Unrepresented dust remains in the mass ledger.
Its carried kinetic energy is excluded from parent light and may be deposited
later by a represented packet. Final solid + packet dust + unrepresented dust
+ ablated mass accounts for the initial mass; status prints the residual.

The bounded process log carries Ablation, Erosion, Breakup, Flare, DustRelease,
Extinction and DarkFlightEnd separately from rendering. Flare records identify
rapid increases of computed power, not added energy. The last two of 64 entries
are reserved for extinction/disposal; omitted entries have a counter. DarkFlightEnd
means local disposal without impact simulation. Diagnostic status shows recent
process entries, mass ledger, packet count and omitted core submissions.

All eight event histories are preallocated at runtime creation and returned to
a pool on expiry, clear, failed initialization or world invalidation. Each has
512 snapshots of eight bodies: at most 3,407,872 bytes of sample storage with
the current Body layout, plus bounded metadata. Spawn performs no allocation
for histories. Samples retain release times and positions so new branches
never interpolate from the planet origin or appear before their birth.

Solid samples compete by received source power for the existing 16-core frame
cap; grain samples only fill remaining slots. At most 32 grain packets can be
active in this implementation, below the 64-packet ceiling. Existing eight
trail points per retained core give separate short wakes; dropped cores reclaim
their trails. Rendering uses the same instanced pipeline and cloud resolve.
`sky meteors spawn fragmenting` selects a brittle, steeper diagnostic entry;
`fireball` retains the seeded material distribution. These presets do not
bypass pressure thresholds. Visual separation and temporal acceptance remain
part of the explicitly gated final verification.


## Residual trains (E047-07)

Fresh fragment wakes cover 0.6 real seconds. Separate f64 deposition rings keep
64 points per solid branch (256 per event), sampled on a 200 ms monotonic grid.
A bounded catch-up fills only the latest ring window after a frame gap. Each
point stores time, physical position, direction, deposited optical energy and
an approximate dust-density sample. Origin changes adapt the same world points;
camera orientation never enters capture or drift. Eight representative points
per retained trail owner fit the existing 128-point frame cap.

Ordinary residual emission expires after three seconds. A seeded subset of
large events has 20..60-second afterglow, exceptionally 90 seconds; at most two
active events may reserve long afterglows. Rare large dust traces have a separate
45-second TTL. Event/history cleanup waits for the last configured train TTL.
The head stops at the physical history end, independently of those timers.
The optical reservoir receives 2% of deposited visible energy, removed from
current emission; each sample decays with cooling and a smooth terminal fade.
Longer/wider lines spread their sampled energy instead of multiplying it.

Available cloud winds describe lower atmospheric decks, not 80 km circulation.
The train therefore uses an explicit bounded upper-atmosphere approximation:
20..60 m/s eastward flow with up to 10 m/s meridional shear, varying with physical
height and seed. Width expands independently for gas and dust. This is neither
a wind solver nor validated atmospheric chemistry.

Trail-only logical owners use zero head radius and independent ids in the same
bounded MeteorFrame. Per-point intensity and kind survive the CPU adapter;
GPU instances carry kind and an endpoint power gradient. Emission and dust share
the same fixed 144-instance, now 9216-byte buffer (64-byte asserted layout).
The shared emission and dust passes each skip their draw when their own count
is zero. No texture or buffer is allocated for a new line. Foreground clouds,
terrain depth and planet occlusion use the existing finite-distance layer.

Dust is premultiplied scattering from physical_sun at its position, including
planetary shadow and finite air transmission; it is suppressed at night.
Residual gas contrast falls in daylight. HDR remains linear to final output;
fallback dust uses the existing scene-space conversion. Counts distinguish
emissive/dust segments; status reports ring points, overwritten captures, skipped
capture steps and both TTLs. Static checks do not establish visual acceptance.

## E047-08: finite-distance visibility and resource order

Standard quality admits finite sources down to the surface. Low quality rejects
head/trail samples below 20 physical km and omits camera-containing cloud proxies.
CPU head culling uses f64 physical coordinates and conservative sphere/segment
occlusion; a hidden head does not discard an independently visible trail. The
meteor shader checks terrain depth and the analytic planet for every fragment,
including near-plane-clipped tracks and observers above the atmosphere.

MeteorFrame carries effective quality from the client environment adapter.
CloudSnapshot supplies the existing face publication and per-cloud transforms.
The client adds half extents and selects at most 16 camera-containing ellipsoids
in stable snapshot order, once per frame. No per-pixel CPU cloud search occurs.
The shared GPU mask compares finite emitter distance with cloud-face distance:
A (camera-cloud-source) attenuates; B (camera-source-cloud) discards the face.
For a source inside a proxy, the face opacity is shortened to the remaining
camera-source path. If camera and source are both inside one selected proxy,
one bounded fullscreen mask draw integrates the partial ellipsoid path instead
of that cloud boundary contribution. Sources beyond its exit use existing faces.
The correction uses the cloud age, opacity and window fade; it adds no second
meteor or dust draw. C (source behind planet) is rejected before composition.

Pass/resource map (frame_encoder::encode_frame, scene_encoder):
1. Shadow depth and atmosphere LUT preparation precede the main scene.
2. Main scene writes scene color and opaque terrain/object depth. Sky and stars
   are included; clouds do not write opaque depth.
3. Atmosphere composes into scene color. Shared emission/dust producers read
   opaque depth and write only finite-layer color, dust and weighted distance.
4. Existing cloud faces write OIT accumulation/revealage, then resolve to scene
   color (or use the existing alpha fallback). Ordinary stars remain behind it.
5. bolid_dust::resolve reads the completed finite distance and opaque depth,
   writes a cleared transmission/foreground pair using cloud faces and the
   optional containing-proxy draw. It then reads those completed textures,
   emission and dust, and writes scene color. No attachment is simultaneously
   sampled; emitter color is added exactly once.
6. UI/output follows, with the existing single HDR tone map.

Bounds: containing proxies <=16 per emitting pixel, one optional draw, no
ray-march steps. CloudUniform is 320 bytes; CloudRecord is 96 bytes, with Rust
offset assertions and Naga layout/binding checks updated together.
Known approximations: ellipsoids smooth holes and deformation in voxel clouds;
excess containing overlaps fall back to boundary faces. This is surface OIT plus
a bounded interior correction, not volumetric radiative transfer. Overlapping
emitters/dust still share one radiance-weighted distance per pixel: separated
depths at exactly the same pixel cannot have independent cloud transmission.
Runtime A/B/C, horizon/orbit and HDR/fallback acceptance remains E047-17.


## E047-09: shared finite-light contract

Binding 11 was free after camera/environment 0..1 and atmosphere 2..10.
EnvironmentGpuBindings now owns one 240-byte uniform, initialized with count=0
and uploaded from each immutable MeteorFrame, including empty/disabled frames.
No event allocates buffers, layouts or bind groups. Four 48-byte sources follow
a 48-byte count/planet/mapping header; Rust and Naga assert size and offsets.
The common group has 12 entries and four uniform buffers, within the existing
downlevel limits; no extra material bind group is needed.

The CPU light insertion scorer rejects expired, out-of-range, planet-hidden,
below-surface and nonlocal (>1000-km altitude) sources before the four-slot cap.
Physical received RGB luminance and ascending IDs determine stable priority.
Cosmetic and Tracked cores use the same input; no celestial input exists.
Radial physical/game mapping uses the manifest surface anchors and inverse
height scale on CPU and GPU, rather than assuming game metres are SI metres.
The finite radius prevents a singularity; a smooth last-20%-of-range fade
bounds the effect. Intensity is the existing visible power divided by 4*pi:
the incident helper divides by physical distance squared only. It emits linear,
unexposed scene values, with analytic planet and bounded atmospheric extinction,
and never modifies global ambient. The observer irradiance diagnostic is a
pre-atmosphere estimate; it is not a GPU measurement.

The first shader consumer is daylight dust scattering in the existing meteor
producer. Count=0 adds exactly zero. The helper is composed only there at this
stage; ground/ocean/cloud integrations belong to the next issues.
Diagnostics expose selected count, observer estimate and actual buffer bytes;
both initial and live GPU allocation totals include the uniform.
No protocol, save, generation or server lifetime change is involved.


## E047-10: local surface lighting

Terrain adds each transient incident term through its existing surface_light
BRDF (material albedo, filtered roughness, specular F0 and normal); the geometric
normal additionally rejects back-facing facets. Authored objects use the same
finite-light irradiance helper with their existing Lambert shading. Ocean adds
a point-source BRDF highlight using its existing roughness/Fresnel and retains
the sky-reflection term. All contributions are linear before tone_map_linear;
no tessellation, extra scene pass or flash-time pipeline compilation occurs.
Sun/Moon, their shadow map and server-bolid lighting remain independent.

The existing 4-source cap applies across active events. Faint presets emit no
light; finite physical distance, core radius, range fade, planet occlusion and
atmospheric attenuation bound bright events. No global ambient term is added.
At this stage cloud attenuation along source-to-surface paths is unity; E047-11
owns that integration. Local transient lights do not cast tree/mountain shadow
maps; geometric normals and planet occlusion are the current self-occlusion.

Use sky meteors debug light off|on for local A/B comparisons. It uploads zero
active transient lights without changing the meteor head, wake, event clock,
Sun/Moon, atmosphere debug or server events. Default is on, session-local only;
sky info reports the switch. Terminal builds parse it and report desktop-only.
The command dispatcher moved its existing portal lookup unchanged into the
gameplay command module to keep the owning files below 500 lines.


## E047-11: cloud light paths

The shared uniform now includes up to eight selected cloud ellipsoids per light
(four on low), 2288 bytes total. All transient-incident consumers use segment
extinction; clouds add directional scattering and independent self-extinction.
The existing weather publication supplies real bases, thicknesses and optical
depth. No generation, mesh cache, worker, Sun/Moon shadow or pass-order change.
See docs/weather.md for bounded costs, source-level A/B reasoning, approximations
and the diagnostic sky meteors debug light transmission mode.


## E047-12: bounded optical halo and HDR path

The existing meteor instance producer marks heads that correspond to selected
transient lights. Their quad expands by at most four sigma (sigma <=0.006 rad),
rather than drawing a fullscreen flash. meteor_optics.wgsl derives a normalized
angular Gaussian from the same head radiance/radius and physical received flux.
A smooth brightness gate suppresses weak sources. Mie/molecular balance,
closest atmospheric altitude, path extinction and observed distance control the
halo; the scattering budget is at most 8% of the lost head energy, additionally
attenuated to the observer. The analytic fallback uses a narrower 0.0015-rad
sigma and 1.5% lost-energy budget. This is a single-scattering approximation,
not plasma spectroscopy, optical bloom or volumetric multiple scattering.

The 40-ms interpolated physical history and continuous brightness gate supply
the same time evolution to emission, local lighting and halo; no frame-random
flash color or global exposure adjustment is added. Faint/no-selected-light
frames execute the previous core/wake path without the halo. Heads hidden by
the planet are disabled; halo pixels additionally check the planet at their
own ray, and terrain depth clips the emitting plane. Shared finite distance
and cloud transmission apply exactly once to the combined head/halo layer.

Resource/exposure map:
- Existing Sun/Moon sky/aerial LUTs retain their original invalidation key.
  Transient lights are not inserted into that key or compute shaders.
- Main surfaces emit linear HDR through tone_map_linear (pre-exposure only).
- Atmosphere composes existing LUT scattering/transmission before the meteor
  layer. The meteor shader applies camera-path transmission and pre-exposure.
- Cloud OIT and finite-layer resolve precede final encode_output tone mapping;
  UI retains its existing separate display-space path.
- Fallback retains the shared finite layer until cloud extinction and tone maps
  emission once in bolid_composite; it requires no atmosphere compute.
  The existing finite-emission target still uses rgba16f; this change does not
  claim support for hardware without that target format.

HDR storage guards reserve <=30000 for the single aggregate server-bolid draw
and <=30000 across all 144 cosmetic instances, preserving RGB ratios. This
prevents emission-target additive half-float overflow; it is not tone mapping.
The final shoulder also bounds its input before the rational curve, avoiding
Inf/Inf after extreme framebuffer values. These guards affect only extreme
overbright values and are not a calibrated camera response or exposure model.
No extra texture/pass or dynamic LUT rebuild is introduced.

sky meteors status and sky info report optical mode, selected sources and
eligible halo-head count. These are prepared counts, not measured GPU pixels.
Runtime orbit/horizon, color and HDR/fallback visual acceptance remains E047-17.


## E047-13: optional delayed audio

Build the desktop client with --features desktop-audio to enable the optional
Rodio 0.22.2 / CPAL adapter. Default desktop and terminal builds do not require
audio. Playback is opt-in per session:
- set client.audio.meteors true
- set client.audio.meteor_volume 0.25
- get client.audio.meteors
- sky meteors status

Properties are typed, local and nonpersistent; reset returns false/0.25. Missing
backend/device reports unavailable while rendering and event physics continue.
Device setup runs on one owned worker; the render thread only uses try_send.
Drop stops and joins the worker and drops all players/stream. Device setup and
shutdown latency depend on the OS backend; no runtime latency is claimed.

Only Breakup events from >=20-kg initial bodies with >=10-GJ estimated fragment
kinetic energy can queue an Airburst cue. Each fragment slot submits at most one,
including when audio was disabled at the event time. Faint/bright presets are
silent. The source position and breakup monotonic timestamp come from the same
physical history. Delay uses full 3D physical source-listener distance and
c=331.3+0.606*15=340.39 m/s (uniform dry air at 15 C). A 50-km path is about147 s;
100 km about294 s. Listener movement after scheduling is not re-integrated;
this is not a terrain/refraction/shock-wave propagation solver.

The preallocated queue holds <=16 cues, max600-s acoustic delay and 2-s lateness
grace. Backend command capacity8 and max4 simultaneous voices are separate hard
bounds. World/context invalidation, confirmed map teleport, clear, disable, volume0,
focus loss and >5-km single-frame listener relocation invalidate queued/playing sound via a
generation stamp. Pending sound may outlive visual head/train lifetime, but not
its world scope. No replicated event, HP change, collision or save field exists.

Original procedural rumble uses seeded filtered noise and a low sinusoid, with
40-ms attack and200-ms end fade,3-s duration,24-kHz stereo. Constant-power panning
and distance attenuation apply; each voice peak<=0.08 before local volume<=1,
so four voices remain bounded. Maximum generated PCM storage is4*144000*f32
=2304000 bytes plus library/device buffers. No recordings or codecs are used.
The adapter is original project code under the workspace license. Dependency
licenses from Cargo metadata: Rodio MIT OR Apache-2.0, CPAL Apache-2.0,
dasp_sample MIT OR Apache-2.0. Platform backend dependencies remain optional.

References for the selected API: https://docs.rs/rodio/0.22.2/rodio/
and https://docs.rs/rodio/0.22.2/rodio/struct.Player.html .
Only the playback feature is enabled; no codec feature bundles are selected.
Game/audio device initialization, listening and runtime validation remain gated
to E047-17; successful builds are not audible acceptance.

Windows x86_64 MSVC release build comparison during E047-13: without audio
19095040 bytes, with desktop-audio 19253248 bytes, delta 158208 bytes
(0.15 MiB). Executables were built but not run; sizes depend
on toolchain/profile/platform and do not measure runtime memory or performance.
