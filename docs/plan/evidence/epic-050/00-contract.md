# E050-00 contract evidence

Issue: [#521](https://github.com/clickbytes/minimium/issues/521).
Epic: [#520](https://github.com/clickbytes/minimium/issues/520).
Deliverable: [execution contract](../../epic-050.md).

## Preparation snapshot

- Source/current main: `ae9d4ab313e38999ac39c17eaabf9d852f93dec9`.
- Report baseline: `3ad46a624010f240eae61db8d33220b8717b08a9` after #519.
- Branch: `work/e050-00-contract`; isolated worktree:
  `D:/Git/minimium/.local/worktrees/e050-00`.
- Initial root checkout and new worktree contained no user modifications.
  Local `.git/info/exclude` ignores workspace worktree directories; this is
  untracked administration, not a changed project policy.
- All 21 issue cards were read from GitHub. The sequence is 00=#521,
  01=#522, 02=#524, 03=#525, 04=#526, 05=#527, 06=#528, 07=#530,
  08=#531, 09=#532, 10=#533, 11=#534, 12=#535, 13=#536, 14=#537,
  15=#538, 16=#539, 17=#540, 18=#541, 19=#542, 20=#543.
  All were OPEN at preparation; #521 has no predecessor. No successor began.

The baseline comparison contains only independent contour work from #529:
`crates/procgen/README.md`, `src/lib.rs`, `terrain_contours.rs`,
`terrain_seam.rs`, new `terrain_zipper.rs`, and E049 evidence
`excavation-contours.md`/`q02.md`. It is retained verbatim. #519's typed
Earth bootstrap validation is present, not an outstanding task to repeat.

## Reviewed sources and resolved boundaries

Read root/crates/client-runtime/server-runtime/apps-client AGENTS,
`architecture.toml`, `docs/architecture.md`, ADR 0014/0015, selected
boundary-review/verify workflows and owner READMEs for gameplay, server-runtime,
client-runtime, procgen, storage, world and client application. `.codegraph`
does not exist; scoped source searches were used without indexing.

| Finding(s) | Current source evidence | Contract resolution |
| --- | --- | --- |
| AR03/02/01 | Server `lib.rs` handler aliases return runtime `ModuleCommandReply`; `commands.rs` keeps payload/failure private; `hosting/commands.rs::RuntimeCommand::parse` wraps private intent | Separate neutral module result, public host data and typed host intent into 01/02/03 |
| AR04/06 | Gameplay `commands.rs` pages expose rows/truncated only; `system.rs` offers count/known-ID access | 04 owns continuation; 05 consumes it for normal-stage discovery |
| AR05 | Gameplay `effects.rs` returns EntityApplied without created ID; system emit has no correlated producer receipt | 06 owns full emit→commit→producer result, including startup |
| AR12/09 | Procgen `terrain_overlay.rs` permits optional incomplete inputs; client `session.rs` owns socket, buffers, JoinHandle, tickets and cache configuration | 07 validates source; 08 owns transport; 09 owns jobs/cache. Logical session retains install/stale checks |
| AR10/08/07/11 | Session lifecycle instants; desktop-gated prediction/readiness in application step; renderer sector/world installation; production stdin adapter | 10 time, 11 CPU readiness, 12 application owner, 13 dispatch remain separate migrations |
| AR14/13 | Storage `excavation.rs::WorldMutation`; coordinator advance waits for matching poll_durable before publication; inventory methods under runtime excavation | 14 neutral plan/persistence first, then 15 moves equipment owner without changing ACK semantics |
| AR15/16 | World component descriptor has replication policy; gameplay schedule embeds concrete built-ins and fixed phase graph | 16 declared mutation authority; 17 neutral validator consuming host plan, preserving default order |
| AR17/18 | RuntimeExecution creates concrete pools; world Inventory has two slots and ToolKind enum | 18/19 are decisions only, not an authorization for speculative frameworks or encoding changes |

Architecture declarations and actual source resolve implementation facts.
Client-runtime README still contains historical protocol v19/v20 and generated
revision 5 prose alongside newer content. Current protocol constant is 21 and
generator validation accepts 6. This existing documentation debt does not permit
a downgrade; affected contract owners update relevant README prose when migrating.
ADR 0014's E048-specific historical serialization is not a new E050 constraint;
E050 itself explicitly requires full 00→20 serialization. Independent read-only
review may assist the coordinator, but dependent implementations remain blocked.

No production API is designed by this document. Proposed consumer names are
explicitly future deliverables. Actual shared-file edits are permitted only for
the card's own producer/consumer migration. Missing predecessor behavior returns
to that predecessor rather than being hidden in a downstream PR. Integration 20
cannot implement missing APIs or create substitute demonstrations.

## Changed files and ownership

Only `docs/plan/epic-050.md` and this report are added. Owner is E050-00;
consumers are each issue implementer/reviewer and final integration 20. The plan
contains all 18 findings, split AR09 transport/content ownership, 21 linked cards,
current declaration/producer/consumer boundaries, compatibility source ledger,
required public/private/documentary evidence, and deferred Q03 scenarios.
No Rust, WGSL, Cargo, architecture declarations, ADR or test policy changes.

## Verification

Commands below are executed in the isolated worktree against the preparation
SHA plus these documentation additions. Final documentation whitespace, local
links and issue sequencing checks completed with the results below. Cargo,
runtime and visual checks remain outside this docs-only verification scope.

| Check | Status / exit | Evidence |
| --- | --- | --- |
| Baseline-to-HEAD `rtk git diff --stat 3ad46a624010f240eae61db8d33220b8717b08a9 HEAD` | PASS / 0 | Seven independent #529 files, 299 additions/96 deletions; no reset |
| `rtk git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-00 diff --check` | PASS / 0 | Both additions included through exact-path intent-to-add; final diff reviewed |
| `rtk proxy python D:/Git/minimium/.local/check_e050_docs.py` | PASS / 0 | Two local Markdown links exist; 21 exact ordered issue URLs and AR01–AR18 coverage validated |
| Cargo fmt/check, feature graphs and source checkers | NOT APPLICABLE | Only two Markdown planning/evidence documents added |
| Remote CI | NOT APPLICABLE if no workflow is triggered | Existing boundary workflow path filters do not select E050 plan/evidence docs; absence is not PASS |
| Unit/integration/E2E, examples/game/GPU/benchmarks | NOT RUN | Explicitly deferred by root policy and Epic |

## Integration gate and remaining work

This branch is not yet a merged/closed issue. Coordinator review, PR/merge SHA,
remote main read-back and closing evidence remain required. Record these in the
issue thread so the report does not need a self-referential post-merge SHA.
#522 cannot begin until #521 is actually CLOSED with its complete merge on main.
Runtime compatibility, CPU prediction, cancellation, durable recovery and visual
behavior remain unverified. Final integration forwards the risk matrix to
existing Q03 #470 while preserving E049 #421 and meteor #384 scope.
