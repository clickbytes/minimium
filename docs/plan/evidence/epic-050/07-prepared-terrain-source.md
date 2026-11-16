# E050-07 / #530: prepared terrain source

Base: `070671e4ba608459df9e4daeba8d1ceeb258bca7` (merged #551).
Branch: `work/e050-07-prepared-terrain` in `.local/worktrees/e050-07`.
Checks below apply to the reviewed implementation diff on this base; the containing
commit and PR identify the delivered tree. Predecessor #528 was CLOSED before work.

## Ownership and construction

`minimium-procgen` owns `PreparedTerrainSource`, `TerrainOverlaySnapshot` and their
validated composition, `TerrainInputs`. Source fields are private; neither source
nor complete inputs has `Default`, an optional-geography conversion, public field
construction or profile/policy replacement builders. Construction validates the
existing manifest contract, exact manifest/map material and relief bindings, and
the required Earth geography/package digest. Dungeon content requires no planet
or geography. No sample policy moved into procgen.

`PlanetGenerationSource` is a separate explicit input for generating a map before
its fingerprint exists. `PlanetTerrain::new_with_relief_policy` retains atomic
generation/surface/definition/geography validation and explicit relief policy.
Runtime sector/render/teleport/volume consumers require complete `TerrainInputs`;
the generation recipe cannot convert into those inputs. `for_manifest` checks the
prepared source's manifest. Map-taking materialization checks its map fingerprint.

Overlays retain the exact global `SectorContentVersion { edits, remnants, volume }`
with the existing revision/last-mutation values, chunk selector and conservative
sector invalidation. Combining overlays and source checks their instance stamps
and the volume store's own `world()`. Existing crater/remnant snapshots carry no
instance ID; the world owner and context-checked client replica supply their
provenance. No new world IDs or revisions were added to those snapshot formats.

The client runtime owns one immutable `ResolvedWorldContent` bundle with private
ticket/manifest/map/source and read-only getters. It does not duplicate mutable
profile/geography fields. `SessionFailure` keeps its concrete error source and
optional `ContentResolutionTicket`, including session/request/context/transition
and content revision. Resolver, join, install, timeout and worker-start failures
retain that association. Existing error code/message equality remains unchanged.

## Producer and consumer matrix

Paths below are relative to the repository. Related symbols sharing a preparation
lane are grouped; this includes all five `ensure_scene` callsites.

| Lane / symbols | Prepared source and overlays |
| --- | --- |
| `games/sample/src/scenario.rs`: `materialize_manifest_map`, scenario planet construction | Explicit pre-map generation source and existing relief recipe; final map identity is bound later. |
| `apps/client/src/world_content.rs`: `bootstrap_terrain_inputs` | Exact local instance/manifest/map, existing application material/relief policy, empty world overlays. |
| Same file: `SampleWorldContentResolver::{resolve_known,resolve_sample}` | Existing geography preparation and fingerprint checks, then `ResolvedWorldContent::prepare`; cancellation retained. |
| `crates/client-runtime/src/world_content.rs`: `ResolvedWorldContent::{prepare,matches}` | Single source binds exact map and ticket instance; matching also checks complete ticket and manifest. |
| `crates/client-runtime/src/session/receive.rs`: `start_world_content_resolution`, `prepare_water_catalog`, `install_resolved_world_content` | Prepared source plus empty overlays for the ticket instance; validates ticket/context/transition/fingerprint before publishing. Water catalog keeps base-terrain semantics. |
| `crates/client-runtime/src/session/excavation.rs`: `terrain_inputs` | Exact active context bundle; only matching ready impact/excavation replica overlays. Returns unavailable until prepared. |
| Same file: `terrain_inputs_for_context` | Shares active state only through the existing active-context predicate; otherwise exact pending bundle with empty destination overlays. |
| `apps/client/src/application/desktop_view.rs`: `scene_terrain_inputs`, `desktop_render_view` | Bootstrap before a session; session active/pending source afterwards. Missing prepared content is `Ok(None)`/omitted pending view. |
| `apps/client/src/application/step.rs`: prediction scene request | Missing source stays pending and rolls back unsupported predicted movement; no default content. |
| `apps/client/src/renderer/init.rs`: `DesktopHost::new` / `ensure_scene` caller 1 | Validated bootstrap inputs; missing Earth geography waits without dispatching a terrain job. |
| `apps/client/src/renderer/session/sector_readiness.rs`: prediction, active and pending scene / callers 2-4 | Inputs from the CPU prediction request, active application source or exact pending view; unavailable content cannot report readiness. |
| `apps/client/src/renderer/session/world_install.rs`: scene installation / caller 5 | Exact pending-context inputs; existing transfer/publication validation retained. |
| `apps/client/src/renderer/sector_scene.rs`: `ensure_scene` | Checks external instance, manifest and source map before cache/worker dispatch. Removed obsolete Earth optional-field guard. |
| `renderer/sector_scene/interests.rs`, `sector_worker.rs`, `collision_worker.rs` | Carry immutable complete inputs; retain revision keys and result/ticket validation. |
| `apps/client/src/renderer/clouds/{runtime,mod,worker}.rs` | Source from active session, explicitly empty overlays; preserves immutable base sampling and existing weather stamp semantics. |
| `apps/client/src/renderer/excavation_debug.rs` and application camera paths | Ready session inputs or existing unavailable handling; no invented terrain. |
| `crates/server-runtime/src/world_registry/accessors.rs`: `prepared_terrain_source`, `prepared_terrain_inputs` | One actual world owner supplies manifest/map/material/relief and current world-local overlays. |
| `runtime/gameplay_terrain.rs`: `build`, `safe_planet_spawn`, terrain adapters | Owner-bound inputs; unchanged optional read-port failure semantics. Destination spawn uses prepared source and its intended overlay selection. |
| `runtime/{altitude,inputs}.rs` | Owner-bound current terrain for altitude and typed operations; unavailable source follows existing rejection path. |
| `runtime/impacts.rs`, `runtime/impacts/spawn.rs` | Owner-bound immutable job inputs; existing job/result world and revision checks. |
| `sector_streaming/workers.rs`: `submit_pending` | Ticket instance selects owner/source/overlays; preparation failure fails that ticket and records typed code plus instance/sector/ticket context. |
| `runtime/excavation.rs`, `excavation/{ports,scene}.rs` | Shared owner-side prepared inputs for current terrain reads and preparation. |
| `excavation/{preparation,impacts}.rs`, `excavation/ports/publication.rs` | Recompose the same immutable source with staged candidate overlays, checked world identity; existing atomic persistence/publication ordering. |
| `excavation/hydrology.rs` | Prepared source from the world content and empty overlays for the existing base water catalog. |
| `crates/procgen/src/{planet_sector,planet_teleport,sector_geometry}.rs` | Every wrapper now requires complete inputs; no `None`/`Default` convenience bypass. Lower-level APIs are content-oriented; hosts enforce the intended world instance. |
| `crates/procgen/src/{excavation_geometry,impact_geometry,volume_base}.rs` | Consume checked source/overlay accessors; volume preparation requires prepared terrain. |
| `crates/renderer/src/{planet_visibility,render_contours,terrain_geometry}.rs` | Same validated runtime inputs and overlay versions; no unbound geography-only renderer construction. |
| `crates/procgen/examples/prepared_terrain_source.rs` | Public procedural generation and prepared source, Earth missing-geography typed refusal, authored dungeon without planet/geography. Compiled only. |

Read-only accessor migration also covers session content access and decode
dispatch, application world-message/desktop operations, and the owning READMEs.
`architecture.toml` documents the actual prepared-source contract. No Cargo
package/dependency or ownership placeholder was added.

## Static source cases reviewed

| Case | Source evidence / result |
| --- | --- |
| Earth without geography | Source constructor returns `Sampling(PackageUnavailable)`; external example handles that concrete kind. No worker dispatch with missing source. |
| Earth package mismatch | Constructor returns `Sampling(PackageMismatch)`; provider failures retain concrete `GenerationError` causes. |
| Different manifest/map profile or relief | Exact existing `matches(manifest, fingerprint)` checks reject construction. |
| Procedural bootstrap | Local policies bind to actual map; explicit relief remains required for generated planets. |
| Authored dungeon | No fake planet or geography; validated material identity remains explicit. |
| Cancelled pending result | Existing cancellation and full resolution-ticket matching remain before install; pending source never replaces active source during preparation. |
| Result after reconnect | Ticket includes session and request identity, context and content revision; stale result cannot match the current request. |
| Overlay change, same geography | Recompose immutable source with new snapshot; independent edit/remnant/volume revisions retain their original meaning. |
| Foreign volume store | `TerrainInputs::new` rejects its own world ID even if the outer snapshot is labelled with the target world. |
| Attempted source/profile replacement | Private fields and no setters; replacement requires a new validated constructor call. |
| Content still resolving | Active inputs absent / pending render view omitted; no readiness success or default geometry; prediction treats it as pending. |
| Cloud and water base sampling | Empty overlays deliberately preserve their prior base-terrain behavior. |

Existing readiness debt: `ClientApplication::transfer_cpu_content_ready` retains
its pre-existing geography requirement for every planet (`planet().is_some()`),
including procedural planets. This change migrates that access to the validated
source but does not change readiness rules or claim procedural-transfer runtime
acceptance. Readiness ownership is a later E050 task.

Protocol version 21, save version 11, generator revision 6, seed/digest arithmetic,
material bytes, coordinate units, cache version meanings and publication order are
unchanged. Independent fixes #519 (Earth bootstrap/readiness) and #529 (contour
zipper) remain in place. No shader, water solver or generator math edits.

The existing client terrain cache remains keyed by instance. Registered server
instance content is immutable in the current owner; same-instance transfers
reuse active overlays only for equal manifest and map fingerprint. The scene
cache invalidates old sector serials and presentations if cached manifest/map
changes. This refactor does not add simultaneous different-content versions of
one instance or redesign the cache for that new capability.

## Verification

Working directory for all commands: `.local/worktrees/e050-07`, base SHA above.
All listed PASS commands were independently executed by the coordinator on the
reviewed implementation tree. No tests or executables were run.

| Exact command (RTK prefix) | Status | Exit / evidence |
| --- | --- | --- |
| `rtk proxy cargo fmt --all -- --check` | PASS | 0 |
| `rtk git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-07 -C D:/Git/minimium/.local/worktrees/e050-07 diff --check` | PASS | 0 |
| `rtk proxy cargo check --workspace --locked` | PASS | 0; all production workspace packages, including desktop client. |
| `rtk proxy cargo check -p minimium-client --no-default-features --locked` | PASS | 0 |
| `rtk proxy cargo check -p minimium-procgen --locked --example prepared_terrain_source` | PASS | 0; public API consumer compiled only. |
| `rtk proxy cargo check -p minimium-gameplay -p minimium-server-runtime --locked --examples` | PASS | 0; earlier external host/effect consumers retained. |
| `rtk proxy python scripts/check_architecture.py` | PASS | 0; all-feature graph and three independent feature graphs. |
| `rtk proxy python scripts/check_feature_contracts.py` | PASS | 0; CPU renderer, terminal client and embedded server graphs and consumers. |
| `rtk proxy python scripts/check_file_lengths.py` | PASS | 0; 777 source files, maximum 500 physical lines. |
| `rtk proxy python scripts/check_static_refactor.py` | PASS | 0; declaration checks only. |
| Unit/integration/E2E suites; game/GPU/benchmarks/examples execution | NOT RUN | Deferred final testing policy; static checks do not prove runtime/visual parity. |

No failed required check remains. Runtime acceptance of transfer cancellation,
reconnect, collision/render parity and real Earth assets remains deferred.

## Exact changed files

```text
apps/client/README.md
apps/client/src/application.rs
apps/client/src/application/desktop_ops.rs
apps/client/src/application/desktop_view.rs
apps/client/src/application/messages.rs
apps/client/src/application/step.rs
apps/client/src/renderer/clouds/runtime.rs
apps/client/src/renderer/excavation_debug.rs
apps/client/src/renderer/init.rs
apps/client/src/renderer/sector_scene.rs
apps/client/src/renderer/sector_scene/interests.rs
apps/client/src/renderer/sector_worker.rs
apps/client/src/renderer/session/sector_readiness.rs
apps/client/src/renderer/session/world_install.rs
apps/client/src/world_content.rs
architecture.toml
crates/client-runtime/README.md
crates/client-runtime/src/session.rs
crates/client-runtime/src/session/content_access.rs
crates/client-runtime/src/session/excavation.rs
crates/client-runtime/src/session/receive.rs
crates/client-runtime/src/session/receive/decode/dispatch.rs
crates/client-runtime/src/session/receive/decode/dispatch/impacts.rs
crates/client-runtime/src/session/receive/decode/dispatch/transitions.rs
crates/client-runtime/src/world_content.rs
crates/procgen/README.md
crates/procgen/examples/prepared_terrain_source.rs
crates/procgen/src/excavation_geometry.rs
crates/procgen/src/impact_geometry.rs
crates/procgen/src/lib.rs
crates/procgen/src/planet.rs
crates/procgen/src/planet_sector.rs
crates/procgen/src/planet_teleport.rs
crates/procgen/src/sector_geometry.rs
crates/procgen/src/terrain_overlay.rs
crates/procgen/src/volume_base.rs
crates/renderer/README.md
crates/renderer/src/planet_visibility.rs
crates/renderer/src/render_contours.rs
crates/renderer/src/terrain_geometry.rs
crates/server-runtime/README.md
crates/server-runtime/src/runtime/altitude.rs
crates/server-runtime/src/runtime/excavation/hydrology.rs
crates/server-runtime/src/runtime/excavation/impacts.rs
crates/server-runtime/src/runtime/excavation/ports.rs
crates/server-runtime/src/runtime/excavation/ports/publication.rs
crates/server-runtime/src/runtime/excavation/preparation.rs
crates/server-runtime/src/runtime/gameplay_terrain.rs
crates/server-runtime/src/runtime/impacts.rs
crates/server-runtime/src/runtime/impacts/spawn.rs
crates/server-runtime/src/runtime/inputs.rs
crates/server-runtime/src/sector_streaming/workers.rs
crates/server-runtime/src/world_registry/accessors.rs
docs/plan/evidence/epic-050/07-prepared-terrain-source.md
games/sample/README.md
games/sample/src/scenario.rs
```
