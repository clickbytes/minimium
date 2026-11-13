# E047 static acceptance and final-verification gate

Implementation reviewed through `6a9b8c14165ad2406635ab851f988d15eae5d958` on
`codex/epic-008-config-cli-console`. The E047-16 commit adds this report,
documentation corrections, layout-checker support and formatting only; its exact
final SHA and command results are recorded in issue #383. Untracked logs are
not source changes and were preserved. No test/game/audio/GPU session was run.

## Eighteen-stage evidence

GitHub state/comments re-read on 2026-11-13; prior closure was not inferred from
checkboxes. All runtime results below are **NOT RUN for this integrated E047 path**.

| Stage / issue | Implementation commit(s) | Acceptance / static evidence | Runtime |
|---|---|---|---|
| 00 Baseline/ADR [#367](https://github.com/clickbytes/minimium/issues/367) | de620e2 | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/367#issuecomment-6719890315), integrated checks below | NOT RUN |
| 01 CPU frame and coordinates [#368](https://github.com/clickbytes/minimium/issues/368) | 81411ac + b577148 + 58bcb6f | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/368#issuecomment-6848207918), integrated checks below | NOT RUN |
| 02 Local runtime [#369](https://github.com/clickbytes/minimium/issues/369) | cf6b4be + e38f295 | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/369#issuecomment-6847802290), integrated checks below | NOT RUN |
| 03 GPU emission [#370](https://github.com/clickbytes/minimium/issues/370) | 8b6dd68 | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/370#issuecomment-6847870356), integrated checks below | NOT RUN |
| 04 Automatic scheduler [#371](https://github.com/clickbytes/minimium/issues/371) | f9cadec | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/371#issuecomment-6847930855), integrated checks below | NOT RUN |
| 05 Ablation/photometry [#372](https://github.com/clickbytes/minimium/issues/372) | b20cdde + c840323 | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/372#issuecomment-6848219592), integrated checks below | NOT RUN |
| 06 Fragmentation [#373](https://github.com/clickbytes/minimium/issues/373) | 32e0a71 | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/373#issuecomment-6848321836), integrated checks below | NOT RUN |
| 07 Residual trains [#374](https://github.com/clickbytes/minimium/issues/374) | 3a8ea03 | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/374#issuecomment-6848559802), integrated checks below | NOT RUN |
| 08 Visibility [#375](https://github.com/clickbytes/minimium/issues/375) | 6d896cb | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/375#issuecomment-6848897736), integrated checks below | NOT RUN |
| 09 Shared point lights [#376](https://github.com/clickbytes/minimium/issues/376) | cf790bb | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/376#issuecomment-6849014736), integrated checks below | NOT RUN |
| 10 Surface BRDF consumers [#377](https://github.com/clickbytes/minimium/issues/377) | 21d3e78 | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/377#issuecomment-6849066907), integrated checks below | NOT RUN |
| 11 Cloud light paths [#378](https://github.com/clickbytes/minimium/issues/378) | 7d3723a | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/378#issuecomment-6849166453), integrated checks below | NOT RUN |
| 12 Optical halo/HDR [#379](https://github.com/clickbytes/minimium/issues/379) | 3262c31 | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/379#issuecomment-6849303362), integrated checks below | NOT RUN |
| 13 Delayed optional audio [#380](https://github.com/clickbytes/minimium/issues/380) | faf22d9 | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/380#issuecomment-6849562532), integrated checks below | NOT RUN |
| 14 Commands and presets [#381](https://github.com/clickbytes/minimium/issues/381) | d75547d | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/381#issuecomment-6849667110), integrated checks below | NOT RUN |
| 15 Budgets/culling/cache [#382](https://github.com/clickbytes/minimium/issues/382) | 6a9b8c1 | CLOSED; [evidence](https://github.com/clickbytes/minimium/issues/382#issuecomment-6849769430), integrated checks below | NOT RUN |
| 16 [#383](https://github.com/clickbytes/minimium/issues/383) | this report commit; exact SHA in issue evidence | static integration review | NOT RUN |
| 17 [#384](https://github.com/clickbytes/minimium/issues/384) | no runtime acceptance commit | OPEN, explicit final-testing gate | NOT RUN |

## Producer-to-consumer source trace

| Edge | Concrete implementation |
|---|---|
| Config / command -> local control | config::resolve_preferences -> ClientOptions meteor_preferences -> renderer/init SettingsStore and MeteorRuntime::configured; CommandRegistry environment::parse -> FrontendAction -> controller -> spawn/apply_property |
| Automatic birth -> event | meteors/spawner::advance -> MeteorRuntime::build_frame -> ActiveEvent::new; no UTC, FOV or server trigger |
| Seed / initial conditions -> physical history | presets::conditions/material -> ActiveEvent::init_basis -> Evolution::initialize -> fragmentation::System::advance -> kinematics::Body::advance |
| History -> core/wake/train/light | presentation::push_event, trains::advance/push -> MeteorFrame push_core/push_trail/push_light; f64 SI -> coords::render_relative_m |
| Prepared environment -> immutable render input | update_render_environment -> PreparedEnvironmentFrame -> RenderFrame.meteors -> RenderSceneSnapshot::meteors |
| Frame -> emission GPU | GpuResources::render -> MeteorGpu::upload -> meteors_culling::visible -> bounded instance buffer -> meteors.wgsl |
| Frame -> local lighting | TransientLightUniform::from_frame -> CloudLightCache::attach -> binding11 -> transient_lighting.wgsl -> world/terrain/ocean/cloud/dust consumers |
| Shared composition -> display | frame_encoder::encode_frame: main scene, atmosphere, bolid_dust::encode, CloudGpu::encode, bolid_dust::resolve, encode_final_passes -> output tone mapping |
| Breakup -> delayed audio | sounds::capture -> MeteorAudio::enqueue/PendingCue -> monotonic tick -> optional bounded Backend worker -> Rodio Player; clear on transfer/teleport/disable |
| World/protocol/save | no edge: cosmetic event data never enter authoritative state; existing server bolid demo remains a separate input sharing composition |

## Static verification and compatibility

Windows MSVC: workspace cargo check, terminal check/build, desktop release build,
optional desktop-audio check/release build; Clippy has 49 pre-existing warnings
and zero errors. Full workspace rustfmt passes after formatting 15 existing
files (imports/wrapping only). check_architecture, check_file_lengths (591 source
files <=500 lines), check_static_refactor, git diff --check and Naga (24 composed
modules) pass. The layout script now ignores line comments before parsing fields
and explicitly compares TransientLight/CloudLightOccluder/TransientLightUniform;
Rust compile-time size/offset asserts and Naga offsets remain independent guards.

Linux GNU terminal cross-check passes. Linux desktop cross-check was attempted
and is BLOCKED by the missing x86_64-linux-gnu-gcc required by the existing
astronomy C bridge. No Linux graphical/audio build or runtime PASS is claimed.
A native Linux desktop build needs its C compiler/window/audio development
libraries (including ALSA for CPAL); native Linux acceptance remains in #384.

Formal scope comparison: git diff 18b970e..HEAD -- crates/protocol crates/storage
crates/procgen crates/world crates/server-runtime apps/server is empty. Public
changes are presentation contracts/renderer adapters and client controls. Protocol
v19, save v10 and generator revision 5 are unchanged. Cargo changes add optional
client-only Rodio/CPAL; pure meteor model and terminal frame contracts remain
GPU-free. No second cosmetic physics implementation or world entity was added.

## Explicit limits and final gate

This is reduced entry physics, not scientific photometry: exponential atmosphere,
sphere/effective-grain cross sections, approximate strengths/efficiencies and
four palettes. No combustion simulation, spectral solver, CFD or physical
implosion. Maximum four solids/four grain packets, finite histories and TTLs
limit detail. Twenty presets are initial conditions, not choreographed outputs.
Cloud ellipsoids fill voxel holes; selection is spatially/temporally approximate.
Surface point lights do not cast tree/mountain shadow maps. The shared finite
emission layer uses weighted distance, so overlapping sources are not independently
occluded. Existing RGBA16F/R16F baseline formats remain required even in low mode.
Sound uses homogeneous 15 C propagation and a listener captured at cue creation.

Before measurement, propose regression limits: idle CPU p95 delta <=0.1 ms;
standard eight-event meteor CPU generation/upload p95 <=2 ms except bounded
initialization reported separately; GPU frame p95 regression <=10% vs identical
scene/seed/baseline, and no monotonic memory growth across 100 spawn/clear cycles.
These are acceptance targets, not measured results or hardware guarantees.
Record any threshold change before measurements with hardware/resolution.

Final matrix: all 20 presets; no-moon/moon night, day/twilight dust; clear/partial
and full cloud decks; source behind/inside/in front of cloud; horizon/terrain
occlusion; ground/orbit, resize1080p/4K, origin rebase, world transfer/reconnect,
timescale0/1/1e6; HDR/analytic low/standard; no audio/device and delayed airburst;
two clients (local cosmetic events need not synchronize). Collect captures and
p50/p95/p99 CPU/GPU/frame/memory evidence from one final SHA. Tests must cover
seed determinism, finite bounds, mass/energy budgets, lifecycle and layouts.

AGENTS.md requires the user to explicitly start final testing. Issue #384 further
gates game/GPU/audio runtime and benchmarks. Without that authorization the
final issue and Epic #366 remain OPEN; there is no GO verdict yet.
