# Earth ground fog and dew

Working tree based on `eb1aa15` on `work/tool-viewmodels`. The following inventory covers this change within a checkout that also contains earlier terrain and tool work.

| Owner | Changed files, relative to the directory in the first column |
| --- | --- |
| `crates/environment` | `README.md`, `src/weather/{climate,condensation,mod}.rs` |
| `crates/renderer` | `README.md`, `src/{atmosphere_debug,atmosphere_gpu,atmosphere_pipeline,atmosphere_resources,atmosphere_uniform,atmosphere_view,fog_relief,frame_encoder,lib,shader_source,shadow_gpu,shadow_target,shadow_uniform,surface_cover}.rs` |
| `crates/renderer/src/shaders` | `{atmosphere,atmosphere_aerial_compute,atmosphere_common,atmosphere_frame,atmosphere_integrate,atmosphere_shadow,atmosphere_sky_compute,shadow_frame,shadow_sampling}.wgsl` |
| `apps/client` | `README.md`, `src/client_commands/{properties,weather}.rs`, `src/client_commands/properties/{catalog,encoding}.rs`, `src/renderer/{gpu,gpu_scene,render,sector_worker}.rs`, `src/renderer/clouds/{mod,runtime,worker}.rs` |
| `crates/localization` | `locales/{en,pl}.ftl` |
| Repository | `architecture.toml`, `docs/{architecture,terrain-materials}.md`, this evidence file, `scripts/wgsl_validator/src/main.rs` |

Environment supplies two UTC weather endpoints and replayed dew. The client publishes the weather grid and the local exterior height tile against their respective source and scene revisions. The renderer integrates direct terrain shadowing and extinction, and composes the same foreground with opaque geometry, clouds and sky. The change requires no protocol, world-generation or save migration.

## Static verification

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS (0) |
| `git diff --check` | PASS (0) |
| `cargo check --workspace --locked` | PASS (0) |
| `cargo check -p minimium-client --no-default-features --locked` | PASS (0) |
| `python scripts/check_architecture.py` | PASS (0) |
| `python scripts/check_feature_contracts.py` | PASS (0) |
| `python scripts/check_file_lengths.py` | PASS (0) |
| `cargo run --manifest-path scripts/wgsl_validator/Cargo.toml --locked` | PASS (0), 25 shader variants |
| `cargo build --release -p minimium-client --locked` | PASS (0) |
| `python scripts/check_static_refactor.py` | FAIL (1) in the full checkout, because recursive source discovery enters other checkouts under `.local/worktrees`. PASS (0) with the same script on the current working tree's Git-listed Rust and WGSL sources, including untracked source additions. |

## Visual acceptance

Game execution, unit/integration/end-to-end suites and GPU measurements are **NOT RUN** under the deferred verification policy.

- Use Standard sky quality on Earth. Compare a humid sheltered valley at dawn, a dry site at the same temperature, and ground wetness after the fog lifts.
- Compare `atmosphere debug fog` and `atmosphere debug fog_shadow` from the valley floor and above the layer. With low sun, the ridge should shade the air below the illuminated top. Use `off` to restore the scene.
- Check camera rotation, sector boundaries, a terrain edit, reconnect and a time jump. Inspect transparency against the sky horizon and test the lunar case separately.
- Compare Low and Standard, from the ground and from orbit. Use `atmosphere status` to collect GPU median and p95 on a timestamp-capable adapter. The height tile describes the exterior surface. Sub-cell hollows and cave airflow are outside its resolution.
