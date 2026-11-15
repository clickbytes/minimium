# E046 source evidence, steps 01–45

Status: source review for E046-01–15; E046-16 reconciles documentation. Every row below is a source-level claim, not a compilation, runtime, performance or visual result. The matching issue report and `docs/architecture-execution.md` contain the detailed traces. Compilation is reserved for E046-17; runtime scenarios require a separately authorized testing phase.

| Step | Owner | Requirement | Producer / current symbol | Consumer and source evidence |
| --- | --- | --- | --- | --- |
| 01 | E046-01 | Preserve dirty entry state | `docs/architecture-issues/E046-initial-git-status.txt` records branch, HEAD and full porcelain state | Current worktree review; `concat.txt` untouched. |
| 02 | E046-01 | Categorize properties | `client_commands::properties` catalog, `TextProperties::execute` | Terminal time uses session frame, movement uses server policy, preferences use `SettingsStore`, presentation reports unavailable. |
| 03 | E046-01 | Guard shadow divisions | `shadow_sampling.wgsl::shadow_visibility` | World, terrain and ocean shader calls keep nine PCF taps and existing bias; numerical guard table in execution doc. |
| 04 | E046-02 | Resolve one application path base | `AppPaths::resolve_from`, `apps/server::run` captured working directory | Server runtime, four entrypoints and child launch receive absolute selected paths; E046-02 report. |
| 05 | E046-02 | Bound diagnostic records and queues | `minimium-diagnostics::QueueSink`, `minimium-telemetry::RunWriter` | Host diagnostic sinks enforce encoded record/field and queue caps; E046-02 report. |
| 06 | E046-02 | Rotate/retain runs | `RunWriter::write_domain_line` and retention owner | Per-run JSONL output uses 16 MiB rotation, seven-day/2 GiB retention; E046-02 report. |
| 07 | E046-02 | Correlate startup and shutdown errors | `StartupStatus` attempt ID, `LocalServer::shutdown` | Desktop/terminal process owners receive typed failure/cause and run ID; E046-02 report. |
| 08 | E046-02 | Preserve domain error causes | `GenerationError`, `SessionFailure::caused_by`, `ServerProcessError` | Sample resolver, server composition and client logs keep stable code/source; E046-02 reopen follow-ups. |
| 09 | E046-03 | Enforce one world manifest contract | `validate_world_manifest`, revision-5 generator and authored fingerprint | Server `WorldRegistry` validates registration; client `manifest_matches_content` checks resolved maps. |
| 10 | E046-03 | Resolve four world kinds | `SampleWorldContentResolver`, `materialize_manifest_map` | Desktop and terminal install verified procedural, Earth, dungeon or authored content by ticket/manifest/fingerprint. |
| 11 | E046-04 | Bound sector tickets | `SectorManager` admission/pin/cancel | `ClientSectorCache::ensure_scene` rolls back pins on replacement failure; sector lifecycle table. |
| 12 | E046-04 | Bound client content preparation | `ContentPreparation` worker/cancel token | Session rejects late content and releases cancelled results; lifecycle table. |
| 13 | E046-04 | Bound server generation | `SectorGenerationPool` workers/queue | Server tick installs at most eight completed results; 1–16 workers and queue 64. |
| 14 | E046-04 | Bound collision builds | `CollisionWorker`, `SceneCollisionWorker` | Client/server install only matching revisions and discard late work; lifecycle table. |
| 15 | E046-04 | Bound material cache | `ContentCache` quota lock and staging cleanup | Installer files only are reclaimed under 8 GiB budget; cache consumers receive failures. |
| 16 | E046-04 | Bound material HTTP | `ContentServer` accept and four worker handles | Queue 32, deadlines, stop/drain and client fetch handling; shutdown detailed in E046-10. |
| 17 | E046-05 | Authenticate before mutation | `AuthenticationProvider::begin/poll/cancel`, `Principal` | Admission identity phase applies one attempt/10 s and active-profile exclusion before activation. |
| 18 | E046-05 | Authorize every mutating message | `dispatch_message`, `has_permission` and world-control handlers | E046-05 issue matrix lists state/context/principal/permission/first mutation and denial events. |
| 19 | E046-05 | Gate developer privileges | `DevProfileProvider`, `ServerPermission` | `--dev` and server-side permissions gate teleport, fly and movement policy; client wire producers only request. |
| 20 | E046-05 | Keep protocol/admission bounds | v18 codec 4096 payload, `dispatch_message` close-deadline gate | Desktop, terminal and loadtest use same wire; decoded frames after Quit do not mutate. |
| 21 | E046-06 | Ack only resolved movement prefix | `Runtime::resolve_movement_input`, `pending_input_seqs` | Snapshot ack cannot cross an earlier executable sample; teleport discards later scheduled samples. |
| 22 | E046-06 | Replay exact sequence/tick revisions | `ClientPrediction::predict_next/reconcile_server` | Client records returned seq/tick; replay checks policy/collision revision and resets on transfer. |
| 23 | E046-07 | Validate component schema/limits | `ComponentRegistry`, `EntityStore` staged mutation | Server world registry applies 2048-byte/16-component/16384-entity/4 MiB limits before commit. |
| 24 | E046-07 | Commit entity and journal atomically | `EntityStore` spawn/upsert/remove/despawn | Replication journal advances with persistent mutation; failed budget leaves prior state. |
| 25 | E046-07 | Save/load complete v10 state | `StorageSnapshot`, `format::load::entities` | Load registers schemas before publication and keeps empty entities, ID counter and initialization marker. |
| 26 | E046-07 | Protect final save from autosave | `PersistenceWorker` capture/revision channel | Final snapshot uses same capture and queues after autosave; E046-07 report and E046-10 shutdown trace. |
| 27 | E046-08 | Schedule deterministic game systems | `EngineModule`, `GameplaySystemContext`, scheduler | Startup New/Restored and tick ordering use dependencies with stable tie-break; E046-08 report. |
| 28 | E046-08 | Apply bounded effects atomically | `WorldEffect` validation and batch commit | Server runtime validates all 256 effects/1 MiB before mutation and journal publication. |
| 29 | E046-08 | Implement portal through public effects | `games/sample::portal_interaction_system`, `GameplaySystemContext::component` | `InteractEntity` reaches component lookup and transfer effect; module has no server infrastructure import. |
| 30 | E046-09 | Track replication per client | `EntitySyncState` pending/acked and journal floor | Two clients plan independently, 4 MiB journal and 64 KiB/tick wire cap; E046-09 trace. |
| 31 | E046-09 | Stage full/delta atomically | `ReplicaAssembly`, `ClientSession::apply_replication_message` | Begin/record/complete validates schema/context/budgets before active publish and ack. |
| 32 | E046-09 | Reset on timeout/interest/transfer | `replicate_entities`, `send_transition_committed` | Pending >30 s restarts full; transfer resets old-world acked and pending state. |
| 33 | E046-10 | One bounded shutdown/final result | `ShutdownRequest`, `lifecycle::shutdown`, `capture_final_snapshot` | Signal/admin/owned child share 30 s deadline, final save result reaches process, final 2 s serve logs. |
| 34 | E046-11 | Typed PL/EN messages and history | `UiMessage`, `Localizer::format`, `ConsoleLine.localized` | Desktop relocalizes retained history; terminal formats on print; fallback PL→EN→key with diagnostic. |
| 35 | E046-11 | Bundle real font and DPI policy | `glyph_atlas::build`, `Console::set_dpi_scale` | RobotoMono.ttf/OFL.txt exist; logical 20 px metrics and physical/logical hit/draw conversion. |
| 36 | E046-11 | Keep grapheme and IME boundaries | `Editor::replace_selection/replace_range`, `Ime::Preedit/Commit` | Console editing/hit testing use graphemes; preedit cannot submit a command. |
| 37 | E046-11 | Prioritize input and persist bindings | `InputMapper::down`, `SettingsStore` | Desktop context filter and focus/context held reset; both frontend config paths validate/save bindings. |
| 38 | E046-12 | Own requested/effective/source/reason | `SettingsStore::SettingRecord` | Desktop/terminal get/set/reset and config show/save read the record; sky fallback reason explicit. |
| 39 | E046-12 | Own application/transfer/overview phases | `ClientApplication`, `WorldTransitionController`, `MapOverviewModel` | Renderer uses methods and CPU `ClientView`; private phase/overview fields prevent direct flag mutation. |
| 40 | E046-13 | Separate CPU preparation/GPU publication | `PreparedShadowFrame`, `ShadowMapPreparationCache`, `GpuResources::render` | Immutable snapshot and Ready/Pending/Failed gate; `Renderer::release_world_sectors` clears CPU/GPU caches. |
| 41 | E046-14 | Match receiver/caster publication | `GeometryPublicationStamp`, `cache_prepared`, `ShadowPass.enabled` | LOD, rebase, transfer and late-result traces; active/expected revision diagnostics separate. |
| 42 | E046-15 | Remove replaced paths | `check_static_refactor::old_paths` and source search | Old client-game, earth generator, atlas generator/binary absent; production callsites use new owners. |
| 43 | E046-15 | Enforce ownership/dependencies | `check_architecture.py::production_graph/forbidden_paths/source_boundary_violations` | Workspace declarations and transitive all-feature normal/build/target edges, source effects and sample ban. |
| 44 | E046-15 | Enforce layouts/size/WGSL | `check_static_refactor.py`, `check_file_lengths.py`, `wgsl_validator` | Actual Rust/WGSL layouts, 500 lines including untracked, current shader variants; execution deferred. |
| 45 | E046-16 | Reconcile code and docs | `architecture.toml`, `docs/architecture.md`, `docs/protocol.md`, this table | Version 18/10/5, principal→permission, input→ack→replay and effect→save/journal→replica traced below. |

## Cross-boundary traces

Authentication: the app sends a bounded provider ID/credential in protocol v18. `AuthenticationProvider::begin/poll/cancel` returns a server-owned `Principal` (`minimium-core::identity`); admission attaches it to the identity only after the attempt and profile checks. `has_permission` checks the principal and records `server.permission_denied` before protected world-control mutation. The sample `dev.profile` provider is a local development claim. Public account authentication and TLS are absent.

Movement: the client predicts at a returned `(input_seq,target_tick)` and records the input with policy/collision revisions. The server admission validates context and permission, queues within the tick window, chooses the newest valid sample per tick, and `resolve_movement_input` advances only the prefix whose samples cannot execute later. Snapshot returns active movement and resolved prefix. `ClientPrediction::reconcile_server` discards that prefix and replays only samples with matching policy/collision revision; transfer resets history and context. Busy without seq is not evidence that one particular sample was removed.

Entities: game modules emit bounded `WorldEffect` values. The server validates the whole batch, component schemas and storage budgets before `EntityStore` mutation; the same commit advances the replication journal. `StorageSnapshot` captures complete entities, including empty ones and ID/initialization metadata, and v10 load validates against registered schemas before publication. Replication plans per-client full/delta from its acknowledged revision, stages the entire image on the client and acknowledges only after complete publication. Errors leave prior active state; cancellation or stale context discards staged work.

Protocol v18, save v10 and generated-world revision 5 reject old versions without migration or compatibility fallback. Use a separate data directory for a new save. Diagnostic domains emit bounded events to process run directories with run IDs, rotation/retention and writer errors as described in E046-02. Source review is complete through E046-15; compilation and runtime are separate states.
