# E050-20 / #543 — integrated architecture and compile-consumer closeout

## Scope and repository snapshot

- Epic: [#520](https://github.com/clickbytes/minimium/issues/520); issue:
  [#543](https://github.com/clickbytes/minimium/issues/543).
- Review baseline: `3ad46a624010f240eae61db8d33220b8717b08a9` after #519.
- Integration worktree: `D:/Git/minimium/.local/worktrees/e050-20`, branch
  `work/e050-20-closeout`, base HEAD `4554472f2f461689385b53348c76bff0888d6039`.
  The worktree began clean. The current change is documentation, architecture
  metadata and checker code only; no production Rust, API, codec, Cargo
  manifest, example implementation, shader or sample behavior is changed.
- `.codegraph/` is absent; source relationships were reviewed from task evidence,
  package READMEs, architecture declarations and current callsites.
- Fresh GitHub reads by the integration coordinator confirmed all twenty
  predecessors #521, #522, #524–#528, #530–#542 are CLOSED/completed and all
  22 linked PRs #544–#565 are merged. Local first-parent history at base 4554472
  contains the entire sequence. No predecessor is claimed from a checkbox alone.
- Independent work is preserved: #519 merge `3ad46a624010f240eae61db8d33220b8717b08a9`
  and contour fix #529 merge `ae9d4ab313e38999ac39c17eaabf9d852f93dec9` precede
  E050. E050 does not claim either change.
- Closeout diff paths: `architecture.toml`,
  `scripts/check_feature_contracts.py`, `docs/plan/epic-050.md`, and this report.
  `.github/workflows/engine-boundaries.yml` already selects both changed catalog
  inputs; no workflow edit is required.

## Integration ledger

Changed files: `architecture.toml`, `scripts/check_feature_contracts.py`,
`docs/plan/epic-050.md`, and this report
(`docs/plan/evidence/epic-050/20-closeout.md`).

The owners below are current source owners at base 4554472. Each example named is
an actual external Cargo target and is now compiled by the feature-contract
catalog. Private-owner tasks are covered by production package checks and their
actual embedded consumer, not by exporting private state for a synthetic API.

| Finding / stage | Issue; merged PR → merge SHA | Owner → producer and connected consumer | Invariant retained; remaining runtime proof |
|---|---|---|---|
| Contract / 00 | #521; #544 `803e5dff45728bc38bfbea4ab1df9bb575aac918`, status correction #545 `c3717f84c9bbdeaee78fc54b5163e0fe80926b6e` | Cross-cutting contract; task evidence and plan delimit owners, compatibility and proof | Migration contract only; no production behavior. |
| AR03 / 01 | #522; #546 `99e09896ce5f6373efb4a5ef58ec8f274608f512` | `minimium-gameplay` owns neutral handler/result; runtime adapts it into the existing registry. `neutral_command_module` is the public consumer. The sample portal is system-only, not a consumer of this command API. | Scope checks, effect commit/drop rules and reply behavior remain runtime-owned; runtime module command behavior not executed. |
| AR02 / 02 | #524; #547 `bb35cc1cb9893ac81c957d123890eec0550d4d99` | `minimium-server-runtime` owns reply state and read-only DTO view; formatter, admin output and `typed_replies` consume it. | Existing payloads, caps, truncation, admin envelope and stop/durability distinction remain; no wire output replay. |
| AR01 / 03 | #525; #548 `ec6538f7da36b0d3ce8d41365d3e3b6baa05c1ff` | Runtime owns typed host intent and shared submission queue; server console/admin text adapters and `typed_host_commands`/`embedded_host` use the host executor path. Client action APIs are separate; this row makes no client frontend parity claim. | No typed-to-string-to-parse execution path or second host queue; host parity not runtime-tested. |
| AR04 / 04 | #526; #549 `2a0b91a768b200b07883eab38807de1b6c20cc91` | Gameplay owns continuation/page contract; runtime world/entity/read-model callers and `paged_reads` consume it. | Bounded rows/bytes, typed stale/error and cursor ownership retained; mutation-during-pagination behavior deferred. |
| AR06 / 05 | #527; #550 `7c50b0bbbeffc57b12a4715a197d6c6a3abb27d9` | Gameplay system context owns ordinary-stage discovery; `world_discovery` and module systems use it without fixed IDs. | Registry remains authoritative; discovery lifecycle not executed. |
| AR05 / 06 | #528; #551 `070671e4ba608459df9e4daeba8d1ceeb258bca7` | Gameplay owns bounded effect receipts and producer correlation; schedule/startup/transfer host composition and `effect_feedback` route results to producer. | Existing bounded batches and owner-lane publication retained; overflow, startup and delivery timing deferred. |
| AR12 / 07 | #530; #552 `7a6dcba5fe4889fca98b1705ea1d90b0ddb99761`, follow-up #553 `e2e229850c343aced5f0d3ef00aa9f1c0a7b1f56` | Procgen owns validated prepared terrain source; world composition/resolvers and `prepared_terrain_source` consume it. | Required Earth geography, map/manifest/fingerprint binding and typed failure preserved; generation/runtime parity not run. |
| AR09-T / 08 | #531; #554 `35480989ca865c8ea3b9b58a86efa0686b489de6` | Client runtime owns transport port; transport crate supplies TCP adapter. Session and `injected_transport` consume the port. | Framing, bounded buffers and existing connector remain adapter-owned; network interruption/cancellation untested. |
| AR09-C / 09 | #532; #555 `172a1b5d6c74cc27c342e3b9655468bff95156f1` | Client runtime owns content-job identity/deadlines; application resolver/default worker and `injected_content_jobs` consume requests/completions/cancellation. | Active/pending identity, stale completion rejection and bounded cache ownership retained; reconnect/late worker behavior deferred. |
| AR10 / 10 | #533; #556 `4d3c7d5662105e2829bd05b71c88241c0062423a` | Client runtime owns logical lifecycle time input; host polling, session/controller and `host_clock` supply/consume it. | Monotonic lifecycle time stays distinct from UTC/world time and physical drain budgets; clock regressions not runtime-tested. |
| AR08 / 11 | #534; #557 `d0f997a84a1b73e87c8ab630b28f47377b37eff6` | Client runtime owns CPU collision snapshots/readiness; application preparation and `collision_ready` consume it independently of renderer view state. | Context/version/coverage binding remains; GPU-pending prediction not executed. |
| AR07 / 12 | #535; #558 `83d29efa104237a666672d827eac395c745096f8` | Client runtime owns public `ClientApplicationCore`; `cpu_client` uses movement, step/reconciliation and immutable view through public API. | Session/world/prediction owner is singular; end-to-end play and prediction parity unverified. |
| AR11 / 13 | #536; #559 `d996b097d12fcfbc93840d3d4930c7d5838fb73c` | `apps/client` owns production stdin capability dispatcher; actual desktop `minimium-client` is compiled. | Loading/disconnected/CPU-ready/GPU-ready capability routing remains frontend-owned; interactive stdin behavior not run. |
| AR14 / 14 | #537; #560 `2099b7f5a82138318d6351ed3406f61182b90f80` | Gameplay owns the neutral world-change contract/plan; storage owns its codec and file/memory adapters, which return typed persistence results. Runtime coordination and `neutral_world_change` consume these contracts. | Validation, ACK and durable-versus-volatile meaning retained; crash recovery and disk outcomes not executed. |
| AR13 / 15 | #538; #561 `496ee4706f7ddc3afb66cb39d1be8f83cf075d1f` | Server runtime privately owns equipment/inventory mutations; combat/admission/replication/restore production paths and `embedded_host` consume it. | Inventory/world transaction and ACK publication remain owner-controlled; deny/restore gameplay cases not run. |
| AR15 / 16 | #539; #562 `e161333c564888eb109daf02db70da6bf972b210` | World owns component mutation policy; runtime registration/effect authorization and `owned_components` consume a second protected component through the private owner. | Schema/payload validation and owner-only mutation remain; denial/restore behavior deferred. |
| AR16 / 17 | #540; #563 `1972371cc67f1a39175d4ec2d0324b45c7a94075` | Gameplay owns neutral plan validation; server runtime maps operation IDs and dispatches retained plan; sample and `host_schedule`, process and embedded hosts consume it. | All eleven prior tick nodes, three stage barriers, eight operations, startup separation, shutdown gate and publication points retained by source review; no tick executed. |
| AR17 / 18 | #541; #564 `0505a160b8f864b04bbe99e7adda710554abc93e` | Decision only: execution-resource split is DEFERRED. | Existing host resource/pool limitation is not fixed or represented as a code pass; separate tick/resource acceptance remains deferred. |
| AR18 / 19 | #542; #565 `4554472f2f461689385b53348c76bff0888d6039` | Decision only: item catalog/compatibility expansion is DEFERRED. | Existing two-tool/two-slot model remains; no extra item or wire/save migration is claimed. |

## Cross-contract source traces

| Composition | Current producer → contract → owner → consumer | Boundary/compatibility evidence |
|---|---|---|
| Neutral command → reply → host input (01→02→03) | `gameplay/src/commands.rs` neutral module handler; `server-runtime/src/commands.rs` execution and `CommandReply::view`; `server-runtime/src/hosting/commands.rs` typed `RuntimeCommand`; `server-runtime/src/runtime/hosted.rs` submission queue; `neutral_command_module.rs`, `typed_replies.rs`, `typed_host_commands.rs`, `embedded_host.rs`. | Module/runtime handlers keep one host executor. Scope and permission validation precede execution; existing reply formatting and queue acceptance semantics remain. Client action APIs are separate and excluded from this trace. See [01](01-neutral-commands.md), [02](02-typed-replies.md), [03](03-typed-input.md). |
| Pagination → ordinary-system discovery (04→05) | `gameplay/src/read_views.rs` cursor/request/page contract; runtime-backed `GameplaySystemContext::worlds`; `gameplay/src/system.rs`; `gameplay/examples/world_discovery.rs` continues pages without fixed world IDs. | Cursor identity/revision and stale errors remain owner-checked; ordinary systems read bounded pages rather than using a second registry enumeration path. See [04](04-pagination.md), [05](05-world-discovery.md). |
| Emit → owner authorization → commit → receipt (06→16) | `gameplay/src/effects.rs` token/result; `gameplay/src/system.rs::emit` and receipt read; `world/src/entity` owner-only descriptor policy; `server-runtime/src/runtime/component_ownership.rs`; `runtime/entity_effects.rs::commit_effect_batch`; `runtime/schedule.rs` producer inbox; `effect_feedback.rs`, `owned_components.rs`. | Owner identity is bound during validated registration; ordinary effects cannot claim owner authority. Capacity is reserved before commit and receipts are correlated to producer; Startup has its separate owner inbox. See [06](06-effect-feedback.md), [16](16-component-policy.md). |
| Prepared terrain → content jobs → CPU readiness → client core (07→09→11→12) | `procgen/src/lib.rs::PreparedTerrainSource`; `client-runtime/src/world_content.rs` validates world/manifest/map identity; `client-runtime/src/session/world_jobs.rs`; `world_readiness` owns collision snapshots; `application/step.rs` and `ClientApplicationCore`; `prepared_terrain_source.rs`, `injected_content_jobs.rs`, `collision_ready.rs`, `cpu_client.rs`. | Active/pending bundle commits together only after identity, fingerprint and readiness checks; stale/cancelled completion cannot publish. GPU presentation readiness does not authorize prediction/world mutation. See [07](07-prepared-terrain-source.md), [09](09-content-jobs.md), [11](11-cpu-readiness.md), [12](12-public-client-core.md). |
| Neutral transaction → equipment owner → ACK publication (14→15) | `gameplay/src/world_changes.rs::WorldChangePlan`; runtime `runtime/equipment.rs` private owner; `storage/src/excavation/journal.rs`/`memory.rs` persistence adapters; `runtime/excavation/coordinator/advance.rs` waits for matching commitment before publication; `neutral_world_change.rs`, combat/admission/replication/restore paths and `embedded_host`. | Gameplay owns the neutral plan contract; storage owns codecs/backends; runtime owns authoritative transaction and ACK lane. Queue submission is not durability, and memory commitment remains volatile. See [14](14-neutral-persistence.md), [15](15-inventory-owner.md). |
| Host schedule → tick barriers and producer IDs (06→16→17) | `gameplay/src/schedule.rs::ScheduleDefinition`/`SchedulePlan`; runtime `commands.rs` builds the mapped plan; `runtime/schedule/host_plan.rs` maps operation IDs; `runtime/schedule.rs` retains producer receipts; `runtime/tick.rs` dispatches process/embedded ticks; `host_schedule.rs`. | Default eleven-node order, eight operations, three barriers and separate Startup remain; producer IDs and receipt boundaries remain stable. Unknown operations fail during composition, before runtime execution. See [06](06-effect-feedback.md), [16](16-component-policy.md), [17](17-host-schedule.md). |

Evidence links for the complete sequence: [00 contract](00-contract.md),
[01 neutral commands](01-neutral-commands.md), [02 typed replies](02-typed-replies.md),
[03 typed input](03-typed-input.md), [04 pagination](04-pagination.md),
[05 world discovery](05-world-discovery.md), [06 effect feedback](06-effect-feedback.md),
[07 prepared terrain](07-prepared-terrain-source.md), [08 transport](08-client-transport.md),
[09 content jobs](09-content-jobs.md), [10 logical clock](10-logical-clock.md),
[11 CPU readiness](11-cpu-readiness.md), [12 public client core](12-public-client-core.md),
[13 stdin dispatch](13-stdin-dispatch.md), [14 neutral persistence](14-neutral-persistence.md),
[15 inventory owner](15-inventory-owner.md), [16 component policy](16-component-policy.md),
[17 host schedule](17-host-schedule.md), [18 execution-host decision](../../epic-050/execution-host-decision.md),
and [19 item-catalog decision](../../epic-050/item-catalog-decision.md).

## Source and compatibility review

The current owner README and implementation boundaries agree with the ledger:
gameplay owns neutral module/schedule and world-change plan contracts; server
runtime adapts typed host commands/replies and coordinates authoritative
execution/publication; procgen prepares validated terrain input; client runtime
owns transport/content-job/session APIs; storage owns persistence encoding and
backend workers; and the renderer remains presentation-only. The
desktop stdin and private inventory tasks are compiled as production owners;
their evidence does not invent public exports to make an example possible.
There are no confirmed missing examples or consumer contracts in the reviewed
predecessor set. The pre-existing `minimium-server-runtime` development edge to
`minimium-game-sample` remains present for fixtures. It is checked separately
from normal/build edges and exposed as the sole narrow dev exception; E050 host
examples themselves use neutral public APIs. The desktop client legitimately
composes `minimium-game-sample`, so its stdin consumer forbids server runtime
ownership, not sample content.

At base 4554472 the independent source reviewer found no compatibility changes
after stage 17: protocol v21, save v11 and generator v6 remain; world item/wire
representations remain two slots/tools; digest labels and FNV inputs, Philox
feeds, world fingerprints, material defaults and terrain bindings remain; the
protected body component keeps ID `0x100`, schema 1, 162-byte encoding; and the
host schedule retains the old eleven-node order, eight operations, three
barriers and separate Startup. These are source/diff conclusions, not byte-level
or runtime execution proof. #519 and #529 remain independent, intact changes.

## Feature contracts and CI integration

`architecture.toml` now catalogs the actual public Cargo consumers for gameplay
(01/05/06/17), server runtime (02/03/04/16 plus embedded host), procgen (07),
client runtime (08–12) and storage (14), plus production desktop stdin (13) and
process-server composition (15/17). Existing renderer CPU, terminal client,
CPU client and embedded-server feature selections remain. The existing
`.github/workflows/engine-boundaries.yml` already runs both architecture and
feature-consumer checkers for `architecture.toml` and `scripts/**`; no duplicate
workflow was introduced.

For example contracts, `check_feature_contracts.py` resolves both normal/build
and normal/build/dev Cargo graphs, validates each named target exists, and
compiles each target separately. It keeps dev-only exceptions explicit, checks
that each is a direct dev dependency, also forbidden in normal/build, and
requires a rationale. The old embedded server's sample fixture edge is
therefore reported, not silently treated as a clean all-target graph. The
desktop production contract allows its documented sample-content dependency.
The process-server contract compiles the actual composition boundary through
runtime, storage, sample content and transport while forbidding client/render
ownership and GPU packages. Compile checks never execute examples.

## Verification ledger

Working tree at base HEAD `4554472f2f461689385b53348c76bff0888d6039`; statuses
are recorded against that source plus the uncommitted closeout diff listed below.
The coordinator ran the final catalog gates against this worktree after adding
all eleven contracts; all 22 consumer invocations passed.

| Command / source SHA | Status | Evidence |
|---|---|---|
| `rtk proxy cargo fmt --all -- --check` @ `4554472f2f461689385b53348c76bff0888d6039` | PASS / 0 | Coordinator baseline; no Rust formatting files changed in this closeout. |
| `rtk proxy cargo check --workspace --locked` @ `4554472f2f461689385b53348c76bff0888d6039` | PASS / 0 | Coordinator baseline. |
| `rtk proxy cargo check -p minimium-client --locked --features desktop` @ base | PASS / 0 | Coordinator baseline. |
| `rtk proxy cargo check -p minimium-client --no-default-features --locked` @ base | PASS / 0 | Coordinator baseline. |
| `rtk proxy cargo check -p minimium-renderer --no-default-features --locked --example cpu_frame` @ base | PASS / 0 | Coordinator baseline. |
| `rtk proxy cargo check -p minimium-server-runtime --no-default-features --locked --example embedded_host` @ base | PASS / 0 | Coordinator baseline. |
| `rtk proxy cargo check -p minimium-gameplay -p minimium-server-runtime -p minimium-procgen -p minimium-client-runtime -p minimium-storage --examples --locked` @ base | PASS / 0 | Coordinator baseline; prior consumers compiled. No examples executed. |
| `rtk proxy python scripts/check_agent_contracts.py` @ base | PASS / 0 | 7 AGENTS / 7 skills / 23 local links. |
| `rtk proxy python scripts/check_file_lengths.py` @ base | PASS / 0 | 819 hand-maintained source files; maximum 500 lines. |
| `rtk proxy python scripts/check_static_refactor.py` @ isolated worktree at base production SHA | PASS / 0 | Root checkout initially failed because it scanned nested archived worktrees and found duplicate historical definitions; the isolated same-source worktree removes that scope artifact. |
| `rtk proxy python scripts/check_architecture.py` @ base + final catalog diff | PASS / 0 | 11 feature graphs; reports dev edges separately. Server-runtime sample appears only through its direct dev edge; normal/build remains sample-free. |
| `rtk proxy python scripts/check_feature_contracts.py --target-dir D:/Git/minimium/target` @ base + final catalog diff | PASS / 0 | 22 invocations passed: 17 unique examples plus terminal, desktop and process-server consumers; two existing examples are also covered by legacy contracts. Existing dead-code diagnostics in three examples only (`injected_transport`, `injected_content_jobs`, `host_clock`). No example executed. |
| `rtk proxy cargo fmt --all -- --check` @ base + closeout diff | PASS / 0 | Coordinator final isolated-worktree check. |
| `rtk proxy cargo check --workspace --locked --target-dir D:/Git/minimium/target` @ base + closeout diff | PASS / 0 | Coordinator final isolated-worktree check; no tests run. |
| `rtk proxy python scripts/check_file_lengths.py` @ base + closeout diff | PASS / 0 | 819 hand-maintained source files; maximum 500 lines. |
| `rtk proxy python scripts/check_agent_contracts.py` @ base + closeout diff | PASS / 0 | 7 AGENTS / 7 skills / 23 local links. |
| `rtk proxy python scripts/check_static_refactor.py` @ isolated worktree/base production SHA | PASS / 0 | Same production SHA; renderer/WGSL structural inputs unchanged by this closeout. |
| `rtk proxy python -m py_compile scripts/check_feature_contracts.py` @ worktree diff | PASS / 0 | Checker syntax only; no Cargo metadata, compilation or application execution. |
| `rtk proxy python -X utf8 -c "import tomllib; from pathlib import Path; tomllib.loads(Path('architecture.toml').read_text(encoding='utf-8'))"` @ worktree diff | PASS / 0 | Architecture metadata parses; no Cargo graph claim. |
| `rtk proxy git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-20 diff --check` @ worktree diff | PASS / 0 | No whitespace diagnostics. |
| Tests, example execution, game/runtime, GPU/visual checks, benchmarks | NOT RUN | Deferred by repository policy; this closeout does not start final testing. |

Coordinator review also verified all 40 local Markdown links and the exact four
staged paths above. `rtk proxy git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-20 diff --cached --check`
passed (exit 0), including the new report. The final independent source/diff
review found no confirmed contract gaps. Remote CI, final merge SHA, main-tree
comparison and the single Q03 handoff permalink are recorded in the
[#543 completion comment](https://github.com/clickbytes/minimium/issues/543)
after integration; no future remote result is claimed by this local evidence.

Root's preliminary `rtk proxy python scripts/check_static_refactor.py` at
`D:/Git/minimium` and HEAD `4554472f2f461689385b53348c76bff0888d6039` failed
with exit 1 when it traversed nested `.local/worktrees` and detected duplicate
archived `ClientGameState` / `GeometryPublicationStamp` declarations. This is
not a production-source failure: isolated worktree rerun at the same production
SHA passed. Both outcomes are retained rather than hiding the initial failure.

## Q03 handoff and completion boundary

The existing [Q03 #470](https://github.com/clickbytes/minimium/issues/470)
remains CLOSED / `not_planned`, as do E049 #421 and meteor #384. Their historical
closure is not runtime acceptance and none is reopened. The coordinator will
append one consolidated comment linking this report and adding all E050 risks
to the prior #470 scope: native/CLI and paging parity; effect feedback, overflow
and startup; transfer cancellation/reconnect/late workers; host clock regression
and UTC divergence; CPU prediction while GPU is pending; inventory/terrain
ACK, failure and restore; owner-only component denial/restore; default tick and
shutdown parity; and wire/save/content compatibility. Preserve existing #470
scenarios: E049/S13/S14/C10, full meteor #384 acceptance, and #523/#529 contour
saved-location `153564.4,431603.2,461399.7 m`, action 4, degenerate transition,
both tools, multi/nonuniform loops, chunk collision and render parity. No GitHub
write is performed by this worktree.

Architecture/static completion means every delivered contract has a connected
consumer and applicable compile/graph gates pass. It does not prove runtime
acceptance, exact serialized-byte parity, disk crash recovery, network
cancellation, gameplay scheduling or visual quality. AR17 and AR18 remain
explicit deferred decisions, not code fixes.
