# E050 execution contract

Source of scope: [Epic #520](https://github.com/clickbytes/minimium/issues/520).
This contract was prepared by [00 / #521](https://github.com/clickbytes/minimium/issues/521)
against `ae9d4ab313e38999ac39c17eaabf9d852f93dec9`. Evidence and source-review
limitations are in [00-contract](evidence/epic-050/00-contract.md).
The review baseline `3ad46a624010f240eae61db8d33220b8717b08a9` is historical,
not a checkout target. All tasks below are OPEN at contract preparation.

## Delivery and sequence

One subagent owns one entire issue in one branch/worktree. Implement sequentially
00 → 20. Before starting a successor, read its predecessor's actual CLOSED state,
linked complete PR, merge SHA and current main containing that merge. A checkbox,
PR creation or compilable unused trait does not satisfy this gate. The coordinator
reviews scope, actual diff, producer/consumer migration and evidence before merge;
after merge, read main again, post evidence and only then close the issue.

| Stage / issue | Finding | Exclusive result | Required predecessor |
| --- | --- | --- | --- |
| [00 / #521](https://github.com/clickbytes/minimium/issues/521) | organization | Migration, ownership and compatibility contract | none |
| [01 / #522](https://github.com/clickbytes/minimium/issues/522) | AR03 | Neutral module command and runtime adapter | #521 CLOSED + merged |
| [02 / #524](https://github.com/clickbytes/minimium/issues/524) | AR02 | Public typed host reply data | #522 CLOSED + merged |
| [03 / #525](https://github.com/clickbytes/minimium/issues/525) | AR01 | Native typed host intent | #524 CLOSED + merged |
| [04 / #526](https://github.com/clickbytes/minimium/issues/526) | AR04 | Bounded continuation for read models | #525 CLOSED + merged |
| [05 / #527](https://github.com/clickbytes/minimium/issues/527) | AR06 | Ordinary-system world discovery | #526 CLOSED + merged |
| [06 / #528](https://github.com/clickbytes/minimium/issues/528) | AR05 | Correlated producer effect receipts | #527 CLOSED + merged |
| [07 / #530](https://github.com/clickbytes/minimium/issues/530) | AR12 | Validated prepared terrain source | #528 CLOSED + merged |
| [08 / #531](https://github.com/clickbytes/minimium/issues/531) | AR09-T | Client transport port and real TCP adapter | #530 CLOSED + merged |
| [09 / #532](https://github.com/clickbytes/minimium/issues/532) | AR09-C | Host content jobs/cache service | #531 CLOSED + merged |
| [10 / #533](https://github.com/clickbytes/minimium/issues/533) | AR10 | Explicit client monotonic lifecycle time | #532 CLOSED + merged |
| [11 / #534](https://github.com/clickbytes/minimium/issues/534) | AR08 | CPU world/collision readiness | #533 CLOSED + merged |
| [12 / #535](https://github.com/clickbytes/minimium/issues/535) | AR07 | Public CPU application/prediction owner | #534 CLOSED + merged |
| [13 / #536](https://github.com/clickbytes/minimium/issues/536) | AR11 | Developer stdin capability dispatch | #535 CLOSED + merged |
| [14 / #537](https://github.com/clickbytes/minimium/issues/537) | AR14 | Neutral change plan and persistence results | #536 CLOSED + merged |
| [15 / #538](https://github.com/clickbytes/minimium/issues/538) | AR13 | Independent equipment/inventory owner | #537 CLOSED + merged |
| [16 / #539](https://github.com/clickbytes/minimium/issues/539) | AR15 | Declared owner-only component mutation | #538 CLOSED + merged |
| [17 / #540](https://github.com/clickbytes/minimium/issues/540) | AR16 | Neutral validator and host-supplied schedule | #539 CLOSED + merged |
| [18 / #541](https://github.com/clickbytes/minimium/issues/541) | AR17 / P3 | Execution-host scope decision, no implementation | #540 CLOSED + merged |
| [19 / #542](https://github.com/clickbytes/minimium/issues/542) | AR18 / P3 | Item-catalog/encoding scope decision, no implementation | #541 CLOSED + merged |
| [20 / #543](https://github.com/clickbytes/minimium/issues/543) | integration | Compile-consumer CI, acceptance ledger, Q03 handoff | #542 and every 00–19 CLOSED + merged |

The numbers missing between GitHub issues belong to independent work. This table
does not claim native GitHub sub-issue/dependency metadata exists.

## Contract ownership and connected consumers

Paths in this table describe current source at the preparation SHA; desired API
names in issues are proposed results. Every successor re-reads its actual main.
Prefixes: `G=crates/gameplay/src`, `S=crates/server-runtime/src`,
`C=crates/client-runtime/src`, `A=apps/client/src`, `P=crates/procgen/src`,
`W=crates/world/src`, `D=crates/storage/src`.

| Contract → exclusive task | Current declaration → producer | Current consumers / migration | Permission for other cards |
| --- | --- | --- | --- |
| Module command → 01 | `S/lib.rs::ModuleCommandHandler/ServerGameplayModule`, `S/commands.rs::ModuleHandler`; neutral context in `G/commands.rs` → installed modules | `initialize_modules`, `execute_registered`, `commands/world.rs`, `hosting.rs::RuntimeModules`, server application composition; sample portal is a system-only existing module, not proof of a neutral command handler | Consume neutral result/handler; 02 alone owns host DTO; 03 alone owns typed host input |
| Host reply → 02 | `S/commands.rs::CommandReply`, private `CommandPayload/CommandFailure` → command executors | `hosting/commands.rs::CommandExecution`, `command_output.rs`, terminal/admin formatting and `embedded_host` | Read DTO only; keep module result neutral, preserve existing adapter text/limits |
| Host intent → 03 | `S/hosting/commands.rs::RuntimeCommand` privately wraps `ServerCommand`; `parse` is public → external host/text parsers | `runtime/hosted.rs` shared submission queue, console/admin execution, embedded host | Construct validated typed intent; no second executor or typed→string→parse |
| Paged reads → 04 | `G/commands.rs::CommandViewPage`, `ModuleCommandReadView`; `S/commands/world.rs::RuntimeCommandReadView` → world registry | `RuntimeQuery/Result`, hosted query, world/entity/player/component listing and formatters | 05 consumes exactly this cursor/error model; no second pagination |
| System discovery → 05 | `G/system.rs::GameplaySystemContext` has count/known-ID reads; `S/runtime/schedule.rs::ModuleSystemContext` → registry | Ordinary stage systems, startup systems and sample module registration | Add consumers only; do not redesign pagination or startup identity |
| Effect feedback → 06 | `G/effects.rs::WorldEffectBuffer/EffectBatchOutcome`, `G/system.rs::emit` → module commands and systems | `S/runtime/entity_effects.rs`, `runtime/schedule.rs`, startup and transfer publication, command reply effects | Later component policy and schedule consume producer IDs/receipts; no new delivery path |
| Prepared terrain → 07 | `P/terrain_overlay.rs::TerrainInputs` permits incomplete optional data → world composers/resolvers | `PlanetTerrain`, sector generation, server terrain read, client world resolver and scene preparation | 09 uses validated result; 11 uses it for CPU geometry; no weaker Earth validation |
| Transport → 08 | `C/session.rs::ClientSession` owns `TcpStream`, buffers/codecs → connector/connected stream | Session poll/queue/receive, controller, desktop and terminal bootstrap | Port and default TCP adapter belong to existing transport crate; 09 must not move framing |
| Content jobs → 09 | `C/session.rs` worker/ticket/cache fields; `session/receive.rs`, `session/content.rs` → world/material preparation | Session install/cancel, `A/world_content` resolver, bootstrap, desktop/terminal shutdown | Port/result in client-runtime, real worker/cache adapter in application; consume 07/08 unchanged |
| Client time → 10 | `C/session.rs` lifecycle `Instant` fields and clock/connector/controller calls → host polling | Session/connection/content timeout, heartbeat, retry, transfer and prediction callers | Inject logical monotonic time; resource-drain instrumentation and world UTC stay distinct |
| CPU readiness → 11 | `A/renderer/session/sector_readiness.rs`, `world_install.rs`, `A/application/step.rs` readiness callback → scene/collision preparation | Prediction, active install, destination readiness, desktop renderer, terminal | CPU collision/world owner independent of FOV/viewport/GPU; 12 composes, does not recreate |
| CPU application → 12 | `A/application.rs::ClientApplication`, `application/step.rs` desktop prediction → actions/messages/transitions | Desktop renderer/controller/camera/window and terminal adapter | Logical owner moves to existing client-runtime; frontend keeps UI/input/locale/audio/presentation |
| Developer stdin → 13 | `A/renderer/window.rs`, renderer event-loop developer input and controller → process stdin | Production capability dispatcher and loading/disconnected/CPU-ready/GPU-ready paths | Private adapter only; no artificial public API, no command parser redesign |
| Neutral change/persistence → 14 | `D/excavation.rs::WorldMutation`, `ExcavationJournalPort` → runtime excavation preparation | File/memory journal, coordinator validate/submit/poll_durable, restore and publication | 15 preserves the transaction/ACK contract; codec remains storage-owned |
| Equipment owner → 15 | `S/runtime/excavation/inventory.rs` plus excavation state → grant/equip/use and restore | Combat/admission, replication, persistence, snapshots, embedded host | Private controlled owner; no codec/transaction rewrite and no public mutable inventory |
| Component policy → 16 | `W/entity/registry.rs::ComponentDescriptor/ComponentReplication` → installed schemas | Runtime entity effect authorization/restore, impact-body component and other systems | Explicit declared owner policy; no hardcoded component exception or altered receipt contract |
| Schedule → 17 | `G/schedule.rs::BuiltinOperation/ordered_schedule` includes concrete mechanics → registry | `S/runtime/schedule.rs::ServerSchedule`, tick dispatch, process/embedded hosts | Host supplies existing operation plan; no new executor, phase or changed receipt timing |
| Execution decision → 18 | `S/hosting.rs::RuntimeExecution` constructs Executor/SectorGenerationPool → process and embedded composition | Existing worker cancellation, drain and resource lifetimes | [AR17 decision](epic-050/execution-host-decision.md): DEFERRED — limitation consciously retained; no alternate execution requirement evidenced; AR17 is not fixed |
| Item decision → 19 | `W/items.rs::ToolKind/Inventory`, sample `excavation.rs` definitions → equipment owner after 15 | Protocol and journal codecs, tool simulation, models and grant/equip/use | Documentation only; distinguish internal stable IDs from new kinds/slots and format migration |
| Acceptance → 20 | Existing `architecture.toml` feature contracts and scripts → delivered consumers | `check_feature_contracts.py`, `check_architecture.py`, `engine-boundaries.yml`, consolidated report | Enforce delivered contracts only; missing API/consumer returns to owning issue |

Shared `lib.rs`, `Cargo.toml`, README, architecture declarations and callsites may
be edited only as the minimum complete migration of that card's contract, on main
already containing its predecessor. The task owner updates exports, README and
actual ownership declarations in the same PR. No parallel edits of shared
contracts, unused placeholders, broad dead-code allowances or deferred caller
connections. A required predecessor API correction returns to that issue for
repair/reintegration before its successor continues.

## Compatibility ledger at the preparation SHA

All entries are preservation requirements. Source review records actual constants
and paths, but does not prove runtime or byte parity. Public Rust API additions do
not authorize wire/save/generator/content migration.

| Invariant | Current authoritative source / value | Required preservation |
| --- | --- | --- |
| Protocol | `crates/protocol/src/limits.rs::PROTOCOL_VERSION = 21`; header 4, payload 4096, frame 4100 bytes | Same messages, IDs, encoding, permissions and adapter envelope |
| Save/journal | `D/format.rs::SAVE_VERSION = 11`; `D/excavation.rs` magic `MEXCAV11`, checkpoint `MEXCHK11`, filename `excavation.v1` | Same codec/replay validation, no fallback or silent migration |
| Generator | `P/generator_contract.rs::validate_world_manifest` accepts revision 6; `generator_digest` label `minimium-procgen/philox4x32-10/planet-sectors/v6-volume-500mm` | Same digest inputs/order, topology tags, position quantum and seed arithmetic |
| Seed/fingerprint | `W/world/model.rs` generation/manifest identity; `W/world/map.rs::map_fingerprint`; journal `codec.rs` writes `world_seed` with `put_u128` | Same map spawn/box/triangle ordering and fingerprint bytes; no random reseeding |
| Sample seed and map identity | `games/sample/src/lib.rs::DEFAULT_SEED = 0x0030_0000_0000_0001`; `scenario.rs` dungeon seed XOR `0x0030_0000_0000_0003`; `W/world/map.rs::map_fingerprint` uses LE FNV-1a over canonical float bits, ordered geometry/flags, domain separators and optional surface; negative zero normalizes to zero | Preserve content seed composition; authored map fingerprint is distinct from generated-code digest |
| Terrain/material bytes | `D/excavation/codec.rs` sample encoding: Empty=0, Soil=1, Rock=2; `WorldMutation` chunk/fluids/craters/catalog fields | Same source bytes, material hashes, chunk coordinates/revisions and catalog identity |
| Material package identity | `crates/assets/src/terrain_material.rs::MaterialManifest::validate` and `docs/terrain-materials.md`: manifest version 1, six ordered roles, BC 4096/RGBA 2048 variants, albedo/normal/packed maps with declared byte counts and SHA-256 | Preserve verified KTX2 bytes, manifest hashes/scales/role ordering and cache identity; preparation ports do not recook content |
| Inventory/IDs | `W/items.rs`: two optional slots, Shovel/Pickaxe; `D/excavation/codec.rs::put_inventory`: u64 revision, equipped none=0/slot+1, item none=0/Shovel=1/Pickaxe=2 | Same model/slots/encoding; 19 documents actual wire/save boundary without implementation |
| Simulation dt | `crates/simulation/src/simulation.rs`: 50 ms / 0.05 seconds; `S/lib.rs::TICK` consumes it, snapshot interval 100 ms | Same fixed integrator, one due tick per step and missed-tick behavior |
| Cooldowns | `games/sample/src/combat.rs::DEFAULT_COMBAT_CONFIG.fire_cooldown_ms = 250`; `games/sample/src/excavation.rs::DEFAULT_TOOL_PROFILE` cooldown 500 ms | Remain application-injected, not newly hardcoded engine defaults; same balance/reach/radius |
| Client lifecycle | `C/session.rs`: welcome 5 s, heartbeat interval 10 s, timeout 20 s; `C/session/content.rs::poll_content` enforces 300 s preparation | Explicit time injection preserves deadline origins, equal/backward handling and ping actual-send semantics |
| Server lifecycle/transfer | `S/lib.rs`: CLIENT_IDLE_TIMEOUT 30 s, CLIENT_HANDSHAKE_TIMEOUT 5 s, CLIENT_AUTHENTICATION_TIMEOUT 10 s, CLIENT_CONTENT_TIMEOUT 300 s; `S/world_registry/transfers.rs::TRANSFER_PREPARATION_TIMEOUT` 300 s | Same admission/transfer expiration and cancellation behavior |
| Shutdown budget | `S/composition.rs::ShutdownRequest::begin_shutdown` creates a 30 s deadline; `S/runtime/loop.rs` subtracts 2 s for service/persistence work, reserving final diagnostics time as documented by `S/lifecycle.rs`. `A/terminal_client.rs` and `A/renderer/window.rs` create 30 s deadlines; terminal cleanup and renderer session worker drain reserve final 2 s. Embedded `ServerRuntime::shutdown` instead accepts the host's absolute deadline | Same endpoint ownership and bounded drain; timeout does not prove interrupted OS I/O |
| Bounds | `C/session.rs`: retained buffer 64 KiB, per-poll read 16 KiB; `S/runtime/hosted.rs`: 64 queued commands; `hosting/commands.rs`: text input 512 bytes, outcome up to 8 executions | No unbounded backlog or increased cap concealing missing continuation |
| Read/effect bounds | `G/commands.rs`: 256 rows / 1 MiB dynamic bytes; `G/effects.rs`: 256 effects / 1 MiB payload per stage; command context emits through supplied `WorldEffectBuffer` | Preserve actual scopes; text reply 8 KiB remains distinct from read/effect and admin JSON caps |
| Reply text | `S/commands.rs::BoundedText::LIMIT = 8 * 1024`, UTF-8-safe literal marker ` [truncated]`; subsequent appends are ignored once truncated | Same bounded text and existing adapter formatting; typed DTO exposure does not change wire replies |
| Tick order | `G/schedule.rs::BuiltinOperation::ALL`, `S/runtime/schedule.rs` and tick dispatch | commands → bolids → impacts → excavation → shutdown gate → world inputs → fixed effects → simulation → environment effects → post simulation effects → clear events |
| Startup/publication | `S/runtime/schedule.rs`: per-world New/Restored startup outside repeated tick | Same stage barriers, initialization markers and owner-lane commits |
| Active/pending/cancel | `C/session.rs::ActiveWorldContent/PendingWorldContent`, `session/transfer.rs`, receive dispatch | Validate session/request/context/transition/manifest/revisions; stale/cancel results never replace active state |
| Commit versus acceptance | `S/runtime/excavation/coordinator/advance.rs::advance/finish_preparation` validates then submits, publishes matching `poll_durable` result | Queued/executed/started/committed/durable remain distinct; failure/rollback and bounded shutdown preserved |
| Memory backend | `D/memory.rs`, `D/excavation/memory.rs`, ADR 0015 | Same typed validation/ACK contract; memory ACK promises volatile retention only |

Earth bootstrap #519 is already integrated. Missing/mismatched geography must
remain a typed failure; 07 cannot make construction succeed by inventing terrain.
Independent #529 replaced mismatched contour stitching with `terrain_zipper.rs`
and seam integration after the report baseline. Preserve that implementation;
E050 does not claim its geometry fix or revert its evidence.

## Required proof per task

These are required future consumers, not claims that they already exist. Cargo
examples are separate crates, use public API and are compiled, never executed.
Record their actual active normal/build/target-specific dependency graphs,
including example dev dependencies; private-import shortcuts and dead stubs fail.

| Stage | Delivery proof |
| --- | --- |
| 00 | This contract/evidence, issue sequence, local links and compatibility source review; Cargo not applicable |
| 01 | `crates/gameplay/examples/neutral_command_module.rs`, real command module without server-runtime |
| 02 | `crates/server-runtime/examples/typed_replies.rs`, all success/failure categories through public DTO |
| 03 | `typed_host_commands.rs` plus existing `embedded_host.rs`, native intents and shared queue |
| 04 | `server-runtime/examples/paged_reads.rs`, continuation, termination and typed stale/error |
| 05 | `gameplay/examples/world_discovery.rs`, ordinary system discovers worlds without fixed IDs |
| 06 | `gameplay/examples/effect_feedback.rs` and host composition, producer token/result, confirmed EntityId, startup result |
| 07 | `procgen/examples/prepared_terrain_source.rs`, valid source and typed required-data failure |
| 08 | `client-runtime/examples/injected_transport.rs`, real session with injected non-TCP port and production TCP consumer |
| 09 | `client-runtime/examples/injected_content_jobs.rs`, request/completion/cancel identity and real default worker adapter |
| 10 | `client-runtime/examples/host_clock.rs`, explicit monotonic input and typed regression handling |
| 11 | `client-runtime/examples/collision_ready.rs`, CPU snapshot/prediction without renderer/FOV/viewport |
| 12 | `client-runtime/examples/cpu_client.rs`, movement→full step/reconciliation→view without desktop/GPU/sample |
| 13 | Compile actual desktop stdin dispatch; capability matrix for loading/disconnected/ready states; no invented public API |
| 14 | `storage/examples/neutral_world_change.rs`, neutral plan, typed outcomes and actual file/memory consumers |
| 15 | Compile combat/admission/replication/restore and embedded consumer through private equipment owner; ownership matrix |
| 16 | `server-runtime/examples/owned_components.rs`, second protected component without central executor exceptions |
| 17 | `gameplay/examples/host_schedule.rs`, real process/embedded dispatch and unchanged default tick |
| 18 | `docs/plan/epic-050/execution-host-decision.md`, confirmed requirement or explicit DEFERRED; no code fix claim |
| 19 | `docs/plan/epic-050/item-catalog-decision.md`, actual codecs/IDs, separate A/B compatibility decisions; no code fix claim |
| 20 | Complete AR/task/PR/merge/owner/consumer/check ledger, existing CI enforcement and one Q03 handoff |

## Integrated status — 2026-11-17

Fresh GitHub reads and local first-parent history confirm stages 00–19 are
CLOSED/completed and merged through #565 at
`4554472f2f461689385b53348c76bff0888d6039`. Stage 20's architecture/feature
gates now PASS on the integration worktree; its ledger is in
[20-closeout](evidence/epic-050/20-closeout.md). The #543 completion record will
capture this branch's review PR, remote CI logs, post-merge main read-back and
issue state once those steps happen; no merge or GitHub write is claimed here.
Q03, E049 and meteor issue states remain as recorded below; administrative
closure is not runtime acceptance.

For each implementation: capture HEAD/worktree; read policy/owner/callers; record
before/after flow and removed paths; review actual diff; run applicable static
checks from the card with command, SHA and exit status. Required FAIL blocks
acceptance; unavailable is NOT RUN with the exact gap. Documentation-only Cargo
checks are NOT APPLICABLE. Checks include diff/fmt, scoped/workspace Cargo checks,
feature consumers and applicable architecture/file-length/static-refactor guards.
No test suites, application/example execution, GPU acceptance or benchmarks until
the user explicitly opens final testing. Hand-maintained source/shaders retain
the 500-physical-line rule. No empty crates or second command/simulation owner.

## Deferred Q03 risk handoff

[Q03 #470](https://github.com/clickbytes/minimium/issues/470) is the historical
runtime-acceptance record. The original E050 description assumed #470, E049 #421
and meteor #384 remained open. Live GitHub states supersede that assumption:
#384 closed at `2026-11-16T09:46:34Z`, #470 at `09:46:37Z` and #421 at
`09:46:50Z`, all with reason `not_planned`, following the user's administrative
closure instruction. [Q03 closure evidence](https://github.com/clickbytes/minimium/issues/470#issuecomment-6912964095)
explicitly records that tests, runtime/GPU, benchmarks and meteor acceptance are
NOT RUN by the agent; these closures are not acceptance or a GO result.
Do not reopen those issues or create a replacement runtime-acceptance issue.
Stage 20 can add its consolidated risk/report comment to the existing closed
#470 without reopening it, preserving earlier E049 and meteor scope as historical
unexecuted requirements. The matrix below remains a risk inventory, not work
authorized for execution by the administrative closure or E050 implementation.

| Later scenario | Contributing stages / unproved property |
| --- | --- |
| Native/CLI parity and paged reads during changes | 01–05: permissions, formatting, cursor restart/progress and membership consistency |
| Effect feedback, overflow and startup | 06/16/17: producer correlation, actual EntityId, denial, bounded delivery and phase timing |
| Transfer/cancel/reconnect and late completion | 07–12: active/pending identity, lease retention, stale worker rejection and rollback |
| Host time equal/backward and UTC divergence | 10: lifecycle monotonic input, ping timing and fixed simulation versus world time |
| CPU prediction with GPU pending | 11–13: CPU coverage, reconciliation, input suppression and loading stdin capabilities |
| Inventory plus terrain ACK/failure/recovery | 14–16: atomic publication, typed backend failure, restore and owner-only mutation |
| Default tick/shutdown parity | 17/18: barriers, cancellation, resource drain deadlines and concrete-pool limitations |
| Wire/save/content parity | All implementations / 19: exact IDs/bytes/defaults/fingerprints, unsupported data rejection |

E050 closes only after all 21 cards are integrated and verified according to
their type. Its final claim is architecture/static complete. AR17/18 may remain
explicitly deferred limitations; runtime acceptance remains NOT RUN in the
administratively closed Q03 record.

## AR18 decision status — E050-19

The decision is `DEFERRED — current two-tool model
intentionally retained`; source and open-issue review found no product request
for more item kinds or slots. AR18 remains unfixed. See the
[item catalog decision](epic-050/item-catalog-decision.md)
for evidence and exact wire/save boundaries.
