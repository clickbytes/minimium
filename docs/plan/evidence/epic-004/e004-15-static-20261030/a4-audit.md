# A4 — E004-15 renderer audit handoff

Baseline: `93b1264502e33f01a9769901fbbc72697bf904e0`.
Commit: `0a7a38ececd9b5164d9225cfed5d8fce40d47522` on `work/e004-15-a4-renderer`.
Worktree: `D:/Git/minimium-e004-a4`, clean after commit.
Status: STATIC PASS for assigned code changes; RUNTIME NOT RUN.

## Delivered fixes

1. `renderer/environment/prepared.rs` holds the immutable CPU frame, finalized environment uniform and optional exactly-500-star array. `environment/runtime.rs:16` measures observer/astronomy/star preparation/frame construction without queue writes. `camera.rs:227` passes the completed package through `window.rs` to `render.rs`.
2. GPU upload occurs only after surface acquisition, before draw use. A skipped acquisition drops the CPU package without updating the GPU star stamp; the next preparation retries when needed. `prepared.rs:34` checks the upload stamp against its frame; `stars_gpu.rs::matches_frame` still gates drawing against the actual uploaded stamp. Failed frame validation drops the prepared stars. `environment/stars.rs` returns a `Box<[StarInstanceGpu; 500]>`, making the upper bound part of the type.
3. Diagnostic version becomes 2. Added distinct upload CPU histogram, p50 to all environment/shadow histograms, explicit sample counts for shadow/upload, and GPU elapsed `n/a`. Optional per-frame upload/shadow duration fields distinguish skipped work from real zero-duration samples. Existing fixed histogram storage and saturating counters remain.
4. Detailed environment memory now includes both dedicated caster buffers (allocated size, not live vertices). Shared world-sector geometry remains counted once in the total engine estimate. `fps show` additionally exposes p50/max and timing sample count for frame intervals and CPU render-loop work.

## Version 2 metric keys and meaning

- `environment_cpu_us_avg/p50/p95/p99/max`, `environment_cpu_samples`: observer adapter, astronomy/cache/star epoch work, optional star-instance preparation and immutable frame/uniform construction. No queue writes. `environment_cpu_scope=observer_astronomy_frame_star_preparation`.
- `environment_upload_cpu_us_avg/p50/p95/p99/max`, `environment_upload_cpu_samples`: CPU cost of the optional star queue write plus camera/environment uniform queue writes. Surface acquisition and shadow work are excluded. `environment_upload_cpu_scope=queue_writes_stars_camera_environment`.
- `shadow_prepare_cpu_us_avg/p50/p95/p99/max`, `shadow_prepare_cpu_samples`: acquired-frame local map caster selection, geometry building, buffer allocation/upload, active terrain caster selection and shadow uniform upload. It excludes remote-player geometry/update work, performed earlier in the general update path. `shadow_prepare_cpu_scope=local_casters_and_uniform_including_upload_excludes_remote_players`.
- These three scopes do not overlap each other. They are nested inside the overall render-loop CPU work, so do not add them to total CPU work as though independent totals.
- `environment_gpu_elapsed_us=n/a`: no timestamp query measurement exists; queue-write or preparation CPU time is never GPU execution time.
- `environment_GPU_map_caster_buffer_bytes`, `environment_GPU_player_caster_buffer_bytes` are added into `environment_GPU_estimate_bytes` alongside existing star/uniform/shadow-texture components. `environment_GPU_shared_sector_buffers=included_in_total_engine_estimate`; `environment_GPU_measurement=engine_estimate_not_driver_VRAM` remains.
- All histogram percentiles use nearest rank and bucket upper bounds; overflow uses observed maximum. Empty histograms report `n/a`. Boundaries remain 1/2/5/10/20/50/100/250/500/1000/2500/5000/10000 us plus overflow. No per-frame diagnostic strings or growing sample lists.
- `fps show`: `frame_ms(avg/p50/p95/p99/max)`, `cpu_work_ms(avg/p50/p95/p99/max)`, and `timing_samples`. These counters remain lifetime cumulative; quality changes reset only the environment histogram/counter epoch and astronomy baselines, as previously documented.

## Source audit findings

- Shared source: `environment/adapter.rs:133–187` takes geocentric Sun independently for the globe, local geometric Sun/Moon for direct illumination and shadows, apparent topocentric vectors for disks. `world.wgsl:24–42` has zero-clamped diffuse terms; `globe.wgsl:51–54` uses geocentric Sun with no local observer-night factor. Water uses the ordinary world geometry/light path; no independent brightness floor remains.
- `environment/adapter.rs:121–132` rotates lunar illumination/pole with the geometric-to-apparent shortest arc, preserving disk orientation. `moon.wgsl` constructs its lit hemisphere from Moon-to-Sun direction, adds foreground atmospheric scattering and uses premultiplied alpha. It draws after stars/Sun, so its entire disk occludes stars; actual appearance is deferred to V15.
- `environment/adapter.rs:436` explicitly transposes row-major f64 to column-major WGSL and accepts finite determinant-minus-one orthogonal transforms. A3 notified us of the Astronomy Engine matrix-layout / handedness fixes; downstream renderer has no determinant-plus-one restriction.
- `camera.rs:181` applies the Earth view reflection once. `environment.rs:87` removes camera translation from the sky view; `stars.wgsl:24–25` uses direction w=0. There is no second sky-specific reflection.
- CPU and WGSL layout fields match (`environment_gpu.rs`, `frame.wgsl`, `shadows.rs`, `shadow.wgsl`, `shadow_sampling.wgsl`). Existing compile-time size/alignment/offset checks enforce camera 208 B, environment 320 B, shadow 128 B; star stride 32 B and total 16,000 B.
- Six cube faces have outward winding/normals (`geometry.rs:96–130`); terrain normals use f64 precise vertices when present and retain source triangle winding (`geometry.rs:68–94`). Culling remains disabled for the reflected view. Visual V16 remains required.
- Exact legacy-light search `rtk rg -n '0\.46\s*,\s*0\.82\s*,\s*0\.34' apps/client/src/renderer` yields no matches. Broader inspection of world/globe geometry/shaders confirms no old global light or water/globe diffuse minimum. `sky_common.wgsl` warm horizon color (including component 0.34), `lighting.rs` altitude/color nodes, star radiance and lunar gain are intentional presentation parameters. Interior ambient and unavailable-state background are intentional; Interior has no direct planetary vectors or sky/shadow pass.
- `quality.rs:5` separates requested/effective quality and preserves failure reason. `shadow_target.rs:5` selects supported sampleable depth format; standard requires Depth32Float. `shadow_target.rs:23` scopes texture allocation/validation failures. `shadow_gpu.rs:137` uses low fallback for initial standard failure; reconfigure builds replacement before replacing current resource. Low retains a 1×1 valid binding texture.
- `shadow_gpu.rs:238` subtracts old/adds new texture estimate and releases dedicated map/player buffers when disabling. `shadow_map_cache.rs:56` clears caster identity/count on release. Rebase (`gpu.rs::set_render_origin`) drains old relative world buffers, invalidates shadow state and marks remote geometry dirty. Resize replaces depth/output targets and adjusts tracked output bytes. These lifecycles need GPU/resource-cycle V18 evidence; none was run.
- Shadows reuse active render-patch geometry regardless of camera frustum; no extra sector loading is introduced. Local caster selection uses 256 m bounds and map upload cap 2 MiB. Missing context/version/data, no source or exceeded map budget clears shadow validity and writes zero uniform (`shadow_gpu.rs::prepare_shadow_frame`). Source selection uses visibility-scaled local light with 5% hysteresis; receiver radius 128 m, fade 112–128 m, 3×3 PCF (`shadows.rs`, `shadow_sampling.wgsl`).

## Static commands actually executed

From the A4 worktree, before commit, with final code (no code changes after these checks):

```text
rtk cargo fmt --all
rtk powershell -NoProfile -Command '$env:CARGO_TARGET_DIR="D:/Git/minimium-e004-a4/target-a4"; rtk cargo check -p minimium-client --locked'
rtk cargo fmt --all -- --check
rtk python scripts/check_file_lengths.py
rtk git -c safe.directory=D:/Git/minimium-e004-a4 diff --check
rtk powershell -NoProfile -Command '$env:CARGO_TARGET_DIR="D:/Git/minimium-e004-a4/target-a4"; rtk cargo clippy -p minimium-client --locked -- -D warnings'
```

All exit 0. Check compiled 131 crates; Clippy reported no issues. Length checker: 249 source files, maximum 500 physical lines. RTK output showed no hard-link/cache or lint warnings in these isolated A4 commands. Full-workspace static/release/allocation-metrics checks are A1's integrated gate, not claimed here.

Before commit the repo identity helper was run with per-process `GIT_CONFIG_COUNT=1`, key `safe.directory`, value this worktree (Git initially refused worktree ownership on this filesystem). Result PASS: profile/account clickbytes, author Pawel Potepa <potepapawel@hotmail.com>, transport confirmed through GitHub CLI. No persistent identity or credential changes. Build outputs were moved after checks to `D:/Git/minimium-e004-a4-target`, after verifying exact source/destination paths, to keep the checkout clean. No user processes stopped.

Changed files: `apps/client/src/environment/diagnostics.rs`, `renderer.rs`, `renderer/{camera,diagnostics,environment,gpu,render,stars_gpu,window,window_metrics}.rs`, `renderer/environment/{prepared,runtime,stars}.rs`, `stats.rs`, `stats/environment.rs`.

## Deferred evidence and handoff

No tests, new test suite, application, loadtest, shader runtime, GPU pipeline creation or visual QA ran. RUNTIME NOT RUN. V13–V18 and UI V19 remain unaccepted; V20 remains NOT MEASURED. V14/V18 must exercise skipped acquisition at a new star epoch, reconnect/context changes and profile cycles with the new prepared upload; V20 must use diagnostic schema 2 and avoid mixing old timing definitions. V13–V17 must also be validated against A3's integrated coordinate corrections. Runtime accuracy/resource/fallback claims still require the authorized E004-16 phase.

## Cross-review correction: cancellation and reconnect cleanup

Read-only review of integration `190fa3e` found a previously missed renderer lifecycle defect. `advance_pending_transitions` removed the prepared target as soon as Ready was queued, while the cancellation handler tried to release resources using the cancellation's source/current context. Thus prepared target pins/GPU buffers survived cancellation. Pending preparation metadata also survived disconnect.

Additional A4 commit: `5d6f350e71865cf6a19bb34140f4bb2fdcfd12b9` (three files: `renderer.rs`, `renderer/session.rs`, `renderer/session/message_handling.rs`). Target metadata now includes its source context and a `ready_sent` flag; after Ready, data is retained without repeating preparation. Accepted Cancel validates stored source context, then releases stored target resources if not active. Commit removes target tracking through existing world installation. Disconnect and new-session start clear pending preparations and release non-active target resources. A2 separately filters stale cancellation events before they reach the renderer in `90fe22dee1b140fd26cb92a61f38e1d525ba1ee3`.

Required V07/V18 regression cases: Prepare→Cancel before Ready; Prepare→Ready queued/partially sent→Cancel; Prepare→Ready→Commit; disconnect/reconnect both before and after Ready; duplicated/stale Cancel. Verify target pins/GPU buffers released on cancel/disconnect, active world remains usable, no repeated target preparation after Ready, no growth over 50 cycles, and source/current cancellation context is never confused with target.

New fix static checks all exit 0: `rtk cargo fmt --all -- --check`, `rtk python scripts/check_file_lengths.py` (249 files, max 500), `rtk git -c safe.directory=D:/Git/minimium-e004-a4 diff --check`; `rtk cargo check -p minimium-client --locked` and `rtk cargo clippy -p minimium-client --locked -- -D warnings` via PowerShell with `CARGO_TARGET_DIR=D:/Git/minimium-e004-a4-target`. Check/Clippy reported one Windows incremental cache hard-link fallback warning, no lint errors. Repo identity helper PASS before commit. No tests/runtime.

The integration review found no other blockers in the metrics-v2 implementation, A3 matrix transpose/handedness consumer path, or the retained TCP queue/probe interaction. `docs/performance.md` accurately describes all metric scopes, sample/skip behavior, memory accounting and cumulative vs quality-reset epochs; `epic-004-final-verification.md` covers V13–V20 and correctly leaves GPU elapsed `n/a` absent timestamps. Add the cancellation regression cases above and schema-2/skipped-new-star-epoch emphasis to the final verification procedure.
