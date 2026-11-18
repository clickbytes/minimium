# Lunar brightness and nighttime exposure

Local implementation on `work/tool-viewmodels` based on `eb1aa15`.

In the supplied 2037×1094 PNG, the terrain at (650, 945) has RGB (237, 225, 193) and the lunar disk at (1275, 465) has RGB (53, 46, 37). Assuming sRGB, the relative display luminance ratio is 26.6 to 1. These samples describe the presented frame. The session log contains no per-pixel light-source or exposure capture.

The previous surface disk used a fixed display gain of 0.08. The previous twilight fill could grow brighter after night gain as the Sun descended. The new receiver illumination estimates the lit atmospheric column above the planetary shadow. The disk and sphere use measured relative lunar flux, astronomical angular size and phase shading. Thin airborne moisture can produce a small lunar corona. Stars and the Milky Way now share the physical scene's pre-exposure convention.

The physical Earth renderer meters 1,024 composited scene pixels on the GPU. It excludes the held item and UI. Absolute exposure adapts in real time. A highlight guard limits overshoot on large scene brightness changes. The final output compensates for HDR storage gain and maps to the existing display surface. Camera adaptation is client presentation state. World data, saves and the wire protocol retain their formats.

## Files

| Owner or directory | Files changed in this implementation |
| --- | --- |
| `apps/client` | `README.md`, `src/renderer.rs`, `src/renderer/{environment/adapter,gpu,render,terrain_status}.rs` |
| `crates/renderer` | `README.md`, `src/{frame_encoder,gpu_frame,held_tool_gpu,lib,moon_lighting,output,output_meter,render_frame,shader_source}.rs` |
| `crates/renderer/src/shaders` | `{atmosphere_common,atmosphere_frame,celestial_halo,clouds,encode_output,exposure_meter,exposure_params,frame,held_tool,lunar_radiance,moon,moon_sphere,ocean,sky,stars,sun,terrain,world}.wgsl` |
| Repository | `architecture.toml`, `docs/terrain-materials.md`, this evidence file and `scripts/wgsl_validator/src/main.rs` |

## Verification

| Command | Result |
| --- | --- |
| `cargo build --release -p minimium-client --locked` | PASS (0) |
| `cargo check --workspace --locked` | PASS (0) |
| `cargo check -p minimium-client --no-default-features --locked` | PASS (0) |
| `cargo fmt --all -- --check` and `cargo fmt --manifest-path scripts/wgsl_validator/Cargo.toml -- --check` | PASS (0 each) |
| `cargo run --manifest-path scripts/wgsl_validator/Cargo.toml --locked` | PASS (0), 26 WGSL variants, including exposure buffer layouts |
| `python scripts/check_architecture.py` and `python scripts/check_feature_contracts.py` | PASS (0 each) |
| `python scripts/check_file_lengths.py` and `git diff --check` | PASS (0 each) |
| `python scripts/check_static_refactor.py` | FAIL (1) in the checkout, with all 115 duplicate-definition reports under `.local/worktrees`. PASS (0) on a temporary copy of the current Git-listed Rust and WGSL sources, including untracked source additions. |

Unit, integration, end-to-end, visual and GPU performance verification are **NOT RUN** under the project's deferred verification policy. Visual acceptance should compare dusk at the same location with and without the Moon, a crescent and full Moon in clear and misty air, a pan from sky to bright ground, and the surface/overview transition. The existing `terrain status` command reports solar altitude, lunar altitude, illuminated fraction, relative flux, storage gain and exposure reference.

## Reflected light on the ground

The physical atmosphere and shadow selector previously each specified a fixed lunar RGB. Its luminance reduced the nominal full-Moon strength to roughly a third. On the analytic path, the disk received the night exposure gain while terrain and water missed it. The adapter now derives reflected irradiance once from the profile's solar luminance, the full-Moon ratio and the ephemeris magnitude. Terrain, water, objects, clouds, atmosphere and shadow selection consume it. Each physical receiver applies its own atmospheric transmission and horizon. The analytic path uses the observer's transmission and disk visibility. The GPU frame gains one 16-byte field with no new pass or texture.

At fixed mean lunar distance and unit heliocentric distance, the bundled Astronomy Engine magnitude law gives the following direct light before extinction or camera exposure. These values are arithmetic evaluations of the model.

| Phase angle | Light relative to full Moon | Light relative to Sun |
| --- | ---: | ---: |
| 0° (full) | 100% | 1.96 × 10⁻⁶ |
| 90° (quarter) | 9.10% | 1.78 × 10⁻⁷ |
| 150° (crescent) | 0.426% | 8.35 × 10⁻⁹ |
| 180° (new) | 0.0281% | 5.50 × 10⁻¹⁰ |

The reference magnitude is −12.74. At the stated distances the ephemeris gives −12.717 at full phase. `terrain status` reports the above-atmosphere Moon/Sun ratio and the fraction of the disk above the observer horizon independently of exposure.

Files in this correction, relative to their owners:

- Client `src/renderer/{environment/adapter,shadows,terrain_status}.rs`.
- Renderer `README.md`, `src/{atmosphere_gpu,gpu_frame,lighting,moon_lighting,profiles}.rs`, and `src/shaders/{atmosphere_common,atmosphere_integrate,clouds,frame,ocean,terrain,world}.wgsl`.
- This evidence file.

At base `eb1aa15`, the release build, workspace and no-default-features client checks, workspace format, file lengths, architecture, WGSL validation (26 variants) and Git diff whitespace checks listed above passed (exit 0) for this correction. The static refactor checker failed in the checkout (exit 1, duplicate declarations in local worktrees) and passed on the current Git-listed sources (exit 0). Tests and visual verification remain deferred. Compare an open slope with adjacent terrain in shadow under full and quarter moons, using the same altitude, atmosphere, material and camera exposure.

## Night camera calibration on Earth materials

The 14-stop exposure ceiling left almost the entire direct lunar contribution on soil within the lowest 8-bit output code. The new ceiling is 21 stops above base exposure in deep night. The HDR histogram selects darker output targets at low incident luminance. The analytic camera bounds its lunar gain using an estimate of horizontal snow illumination. Daylight retains its previous metering targets and the source strength and phase law retain their previous calibration.

These are calculated sRGB values for a uniform diffuse surface filling the frame, at mean lunar distance, lunar elevation 30°, assumed neutral atmospheric transmission 0.8 and base exposure 1.25. Albedos are the 1×1 per-layer mips from the resident Earth material asset `target/materials/sample-v1/albedo-2048.ktx2`. The calculation includes linearization, the solar luminance and lunar tint, the projected cosine, the two percentile bands of the half-stop histogram, settled exposure, the output curve and sRGB encoding. It isolates direct light and excludes ambient, occlusion, specular, fog and horizon scenery. It is independent arithmetic, not a GPU frame capture.

| Material | Full, previous | Full, corrected | Quarter, corrected |
| --- | --- | --- | --- |
| Soil | (1, 0, 0) | (59, 44, 31) | (13, 8, 5) |
| Grass | (0, 1, 0) | (36, 47, 16) | (8, 13, 3) |
| Litter | (0, 0, 0) | (52, 36, 22) | (6, 4, 2) |
| Snow | (12, 18, 29) | (48, 63, 91) | (38, 50, 74) |

This adjustment changes `crates/renderer/src/{render_frame,lighting}.rs`, `crates/renderer/src/shaders/exposure_meter.wgsl`, the renderer README and this record. It uses the existing exposure buffer, compute dispatch, frame uniforms and presentation contracts.

Static results for this adjustment on `work/tool-viewmodels`, base `eb1aa15`:

| Command | Result |
| --- | --- |
| `cargo build --release -p minimium-client --locked` | PASS (0) |
| `cargo check --workspace --locked`, `cargo check -p minimium-client --no-default-features --locked` | PASS (0 each) |
| `cargo fmt --all -- --check`, `git diff --check` | PASS (0 each) |
| `cargo run --manifest-path scripts/wgsl_validator/Cargo.toml --locked` | PASS (0), 26 shader variants |
| `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` | PASS (0 each) |
| `python scripts/check_static_refactor.py` on current Git-listed sources | PASS (0) |

## Accelerated dusk and moonrise

On `work/tool-viewmodels` at base `eb1aa15c27d0a81f7096e553101e43323464ffda`, the server supplies the world clock. The client estimates presentation UTC, prepares the sky for that instant and uploads one environment for the frame. Several downstream conversions produced the reported delay.

- Session `000046` recorded 58 clock smoothing timeouts at the accelerated setting. The old convergence threshold was half a world millisecond. At 1000× that represents only half a microsecond of wall time, below the resolution of the network timestamps. The threshold now accounts for the clock rate. For an isolated 2 ms real offset, the smoothing curve reaches the threshold in approximately 2.77 s at 1000×, previously 16.59 s. The remaining correction drains at the normal slew limit. Large corrections and genuinely prolonged drift still trigger resynchronization.
- The point-star catalog and galactic map previously paid both the authored twilight fade and a fixed deep-night exposure divisor. Both are now calibrated to the frame's exposure reference. At that exposure, the clear-sky stellar signal is about 6.7% of its deep-night value at a solar altitude of −6° and 46% at −12°. The old calculation gave about 0.032% at −12°. Individual magnitudes, diffuse map brightness, air, cloud and camera exposure still influence the resulting pixels.
- The HDR meter now compares elapsed presentation UTC with elapsed monotonic time. Under acceleration it uses the change in incident light at the observer and any binding twilight exposure ceiling to follow the corresponding change measured in the image. It samples the same solar and lunar irradiance functions as terrain. Turning the camera under steady illumination still uses real-time adaptation. A change of world, view, clock revision, reverse UTC or a wall-time gap of at least a second reacquires the target.
- The Sun and Moon use their astronomical apparent size in the surface sky. The receiver's local horizon, refracted ray and planetary occlusion govern the finite disk's direct light. At sea level a geometric elevation of −0.5° gives an apparent center near +0.06° in the current model. Lunar phase and extinction still scale the light. For the clear sea-level column below, horizontal direct full moonlight at 5° and 10° is about 3.0% and 9.6% of its zenith value. The analytic Earth path uses the spherical geometry when present. The lookup accounts for rays entering the atmosphere from orbit and for unobstructed vacuum paths.

### Exposure calculation

An independent calculation used a uniform flat soil frame, clear Earth air, optical radius 6500 km, full Moon at mean distance, 60 Hz presentation and solar descent of 15° per simulated hour. The Moon rises opposite the Sun at the same rate. The calculation includes the soil mip RGB (77, 57, 36), optical extinction, projected incidence, ambient light, half-stop histogram, twilight exposure ceiling and asymmetric adaptation. The table gives stops of exposure at the same solar elevations. The 1× series interpolates lighting samples spaced 16.67 world seconds apart to each display frame. Values are approximate model outputs.

| Sun altitude | Meter target (EV) | 1× | Previous 1000× | Corrected 1000× |
| --- | ---: | ---: | ---: | ---: |
| −12° | 10.732 | 10.699 | 2.439 | 10.723 |
| −15° | 17.970 | 17.945 | 5.052 | 17.957 |
| −18° | 20.560 | 20.553 | 8.223 | 20.511 |

Following incident light alone gave 7.167 EV at −12°. The change in the twilight ceiling accounts for the rest of the corrected response. In a separate dark-sky moonrise with the Sun fixed at −32°, the corrected 1000× estimate at lunar altitude 15° was 20.96 EV for a target of 20.81 EV. The eye can still differ from the target as the image changes.

### Owners and files

| Owner | Files in this correction |
| --- | --- |
| Client clock | `crates/client-runtime/{README.md,src/session/clock/smoothing.rs}` |
| Astronomy | `crates/astronomy/{README.md,src/frames.rs,src/lib.rs}` |
| Desktop client | `apps/client/README.md` and `apps/client/src/renderer/{camera,celestial_labels,diagnostics,environment/adapter,environment/halo,gpu,render,shadow_prepare,shadows,sky}.rs` |
| Renderer | `crates/renderer/README.md` and `crates/renderer/src/{atmosphere_gpu,atmosphere_view,frame_encoder,gpu_frame,output,output_meter,profiles,render_frame,shader_source}.rs` |
| Renderer shaders | `crates/renderer/src/shaders/{atmosphere_common,atmosphere_frame,atmosphere_integrate,celestial_halo,clouds,exposure_meter,exposure_params,frame,moon,moon_sphere,ocean,sky,sky_common,stars,sun,terrain,world}.wgsl` |
| Validation and evidence | `scripts/wgsl_validator/src/main.rs` and this file |

The meter reuses the existing dispatch and frame bindings. Its parameter buffer is 48 bytes and its adaptation history is 32 bytes. This change is local to astronomy and client presentation, with no wire, save, asset or generator migration.

Final visual acceptance is pending. Compare clear dusk with a fixed camera at 1× and 1000× at matching solar elevations, then pan at paused world time. Compare a full and quarter Moon at matched elevations against terrain in direct light and beyond a ridge. In `sky info`, compare `world_utc` and `render_minus_estimated_utc_ms` at the same moment. A fresh accelerated session log will establish whether timeouts recur.

| Verification for this correction in `D:/Git/minimium` | Result |
| --- | --- |
| `cargo build --release -p minimium-client --locked` | PASS (0), rebuilt `target/release/minimium-client.exe` |
| `cargo check --workspace --locked` and `cargo check -p minimium-client --no-default-features --locked` | PASS (0 each) |
| `cargo fmt --all -- --check` and `cargo fmt --manifest-path scripts/wgsl_validator/Cargo.toml -- --check` | PASS (0 each) |
| `cargo run --manifest-path scripts/wgsl_validator/Cargo.toml --locked` | PASS (0), 26 shader variants and exposure layouts |
| `python scripts/check_architecture.py` and `python scripts/check_feature_contracts.py` | PASS (0 each) |
| `python scripts/check_file_lengths.py` and `git diff --check` | PASS (0 each) |
| `python scripts/check_static_refactor.py` | FAIL (1) in the checkout with duplicate declarations from local worktrees. PASS (0) on the current Git-listed source files, including untracked additions. |
| Game, GPU visual/performance checks, unit, integration and end-to-end suites | NOT RUN under the project's deferred verification policy |

## Moonset at a planetary horizon or ridge

Implemented on `work/tool-viewmodels` at base `eb1aa15c27d0a81f7096e553101e43323464ffda` in the existing working tree of `clickbytes/minimium`.

| Iteration | Implementation and static verification |
| --- | --- |
| Diagnostic controls | The desktop console and typed properties expose direct moonlight, diffuse lunar skylight, other lighting, map coverage and a simultaneous RGB view. The same frame can be compared with absolute exposure held and the prepared lunar flux set to zero. Client compilation and the WGSL validator passed. |
| Direct visibility | Standard retains matching lunar contact and landscape projections across primary-light changes. The finite disk uses the receiver’s planetary horizon and a bounded ridge estimate from published geometry. An unrepresentable bolid point projection leaves the directional maps eligible. Client compilation and WGSL validation passed. |
| Diffuse irradiance | The fixed 12% term was replaced by a 65×17 unit-flux table computed alongside spherical optical depth. Local terrain horizons attenuate the diffuse sample. Fog integrates lunar in-scattering with view-path extinction. Both Earth paths share the table. Client compilation and WGSL validation passed. |
| HDR and time | Shadow depth, atmospheric view tables, surface shading and the incident-light meter use one frame and caster publication. On a normal HDR scene the exposure lock overrides display gain while the histogram and adaptation continue. Debug channel views keep their chosen exposure outside the histogram. Teleport clears previous-view exposure history. Workspace compilation and WGSL binding and layout checks passed. |

The table integrates single-scattered sky radiance with the projected area of each incident direction. Its inputs are the optical profile, receiver height and source elevation. The current frame supplies lunar phase and distance through the existing irradiance calculation. Air without scattering coefficients produces zero diffuse irradiance. The cosine-weighted hemisphere integral follows the indirect irradiance formulation in the plan. [Bruneton’s irradiance derivation](https://ebruneton.github.io/precomputed_atmospheric_scattering/atmosphere/functions.glsl.html#irradiance)

Standard reserves 8 MiB of additional depth storage and 256 additional uniform bytes for the two concurrent lunar cascades. Identical contact coverage during a takeover avoids switching the valley floor to a different texel scale. The irradiance table adds 8,840 bytes on either Earth path. The CPU sector worker uses the spare relief channel for eight-direction sky obstruction. The depth resolution, vertical silhouette approximation and published terrain detail bound ridge accuracy. Low quality can resolve blockers only within its local relief tile. GPU cost and the apparent cutoff require final measurements.

### Final capture matrix

With the Sun deeply below the horizon, keep the camera, material, UTC and weather identical within each comparison. Capture the open shore with water, the valley floor, and the exposed peak before occultation, at the last sliver, just after disappearance, and several degrees below the horizon. Compare full and quarter phases, clear air, fog and zero lunar flux at 1× and 1000×. The priority pair is the valley immediately before and after the final sliver, recorded at the same fixed exposure.

Use `terrain debug light_channels` to read direct lunar, diffuse lunar and other luminance in red, green and blue. Use `moon_coverage` for the contact and landscape weights. Record `terrain status`, then compare the normal image at fixed and settled automatic exposure. Check a Sun/bolid takeover, rotation at paused UTC, a UTC jump, and observer relocation for stale illumination. Check the GPU timing at matched geometry and weather. Captures, performance measurements, and unit, integration and end-to-end suites are **NOT RUN** under the project’s final-verification gate.

### Static checks

All commands ran from `D:/Git/minimium` unless indicated. The original release executable was in use by a running client.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` and `cargo fmt --manifest-path scripts/wgsl_validator/Cargo.toml -- --check` | PASS (0 each) |
| `cargo check --workspace --locked` and `cargo check -p minimium-client --no-default-features --locked` | PASS (0 each) |
| `cargo run --manifest-path scripts/wgsl_validator/Cargo.toml --locked` | PASS (0), 26 variants including lunar sampling bindings and exposure layout |
| `python scripts/check_architecture.py` and `python scripts/check_feature_contracts.py` | PASS (0 each) |
| `python scripts/check_file_lengths.py` and `git diff --check` | PASS (0 each) |
| `python scripts/check_static_refactor.py` | FAIL (1) in the checkout. All 115 duplicate-definition reports reference `.local/worktrees/`. PASS (0) in a temporary copy of this checker and the working tree’s Git-listed Rust and WGSL files, including untracked sources. |
| `cargo build --release -p minimium-client --locked` | FAIL (1), Windows denied replacement of the running executable |
| `cargo build --release -p minimium-client --locked --target-dir target/moonset --config 'build.build-dir="D:/Git/minimium/target"'` | PASS (0). The updated executable is `target/moonset/release/minimium-client.exe`. The existing cooked material manifest is `target/materials/sample-v1/manifest.ron` and can be supplied through `MINIMIUM_MATERIAL_MANIFEST`. |

### Files changed for moonset

Client presentation and the renderer own the changes. Server authority, world generation and protocol, save and asset formats retain their existing contracts. Paths below are relative to the directory in the first column.

| Directory | Files |
| --- | --- |
| `apps/client` | `README.md` |
| `apps/client/src/application/settings` | `startup.rs` |
| `apps/client/src/client_commands` | `properties.rs`, `weather.rs` |
| `apps/client/src/client_commands/properties` | `catalog.rs`, `encoding.rs`, `types.rs` |
| `apps/client/src` | `renderer.rs` |
| `apps/client/src/renderer/environment` | `runtime.rs` |
| `apps/client/src/renderer` | `gpu.rs`, `init.rs`, `render.rs`, `settings.rs`, `shadow_prepare.rs`, `shadows.rs`, `terrain_status.rs` |
| `apps/client/src/renderer/session/message_handling` | `gameplay.rs` |
| `apps/client/src/renderer/settings` | `values.rs` |
| `apps/client/src/terminal_client` | `properties.rs` |
| `.` | `architecture.toml` |
| `crates/localization/locales` | `en.ftl`, `pl.ftl` |
| `crates/renderer` | `README.md` |
| `crates/renderer/src` | `atmosphere_gpu.rs`, `atmosphere_resources.rs`, `atmosphere_table.rs`, `fog_relief.rs`, `frame_encoder.rs`, `lib.rs`, `output.rs`, `output_meter.rs`, `shader_source.rs`, `shadow_gpu.rs`, `shadow_target.rs`, `shadow_view.rs`, `sky_irradiance.rs`, `terrain_debug.rs` |
| `crates/renderer/src/shaders` | `atmosphere_common.wgsl`, `atmosphere_frame.wgsl`, `atmosphere_integrate.wgsl`, `atmosphere_shadow.wgsl`, `encode_output.wgsl`, `exposure_meter.wgsl`, `exposure_params.wgsl`, `ocean.wgsl`, `shadow_frame.wgsl`, `shadow_sampling.wgsl`, `terrain.wgsl`, `terrain_material.wgsl`, `world.wgsl` |
| `docs/plan/evidence` | `lunar-exposure.md` |
| `scripts/wgsl_validator/src` | `main.rs` |
