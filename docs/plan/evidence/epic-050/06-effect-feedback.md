# E050-06 correlated effect feedback evidence

## Provenance

- Repository: `clickbytes/minimium`
- Issue: [#528 E050-06](https://github.com/clickbytes/minimium/issues/528)
- Prerequisite: #527 CLOSED; [PR #550 CLOSED](https://github.com/clickbytes/minimium/pull/550)
- Base `HEAD`: `7c50b0bbbeffc57b12a4715a197d6c6a3abb27d9`
- #550 merge SHA: `7c50b0bbbeffc57b12a4715a197d6c6a3abb27d9`, present in `origin/main`
- Worktree branch: `work/e050-06-feedback`
- Current source owner: `minimium-gameplay` owns neutral producer tokens/receipts; `minimium-server-runtime` owns registered producer identity, bounded delivery, commit and world entity allocation.

## Before and after

| Producer / phase | Before | After |
| --- | --- | --- |
| Ordinary system | Stage-wide bare effects were committed once; system identity was not retained and only diagnostics saw outcomes. | The runtime binds each writer to registered module/system IDs and a persistent sequence. `emit` returns its accepted token. The same system can consume its prior results on its next call. Unread results expire after that call and are counted/logged. |
| Startup | Runs once per world and then the temporary stage batch disappears; there was no host result reader. | Producer identity includes module, system and world. A bounded 4096-entry owner inbox is drained with `ServerRuntime::take_startup_effect_receipts`; Startup is not invoked again. Unread receipts are included in `ShutdownOutcome`. |
| Module command | Caller received ordered typed per-effect outcomes, without producer tokens. | Producer identity includes registered module and command; sequence state persists. The command caller receives ordered correlated receipts directly. Handler errors still discard effects from commit and attach `StageRejected` receipts while preserving the original rendered error. No command inbox duplicates caller results. |

Each producer sequence stops at exhaustion and never wraps. Receipt slots are
reserved before an effect is accepted. Ordinary receipts are capped at 256 per
producer and 4096 total pending plus reserved. Startup receipts have a separate
4096 total pending plus reserved cap. Backpressure rejects only the attempted
effect; it does not convert to shared stage-buffer overflow or reject other
producers' effects. Counts are exposed on `ServerRuntime` and diagnostics.

The stage still uses one `WorldEffectBuffer` with its existing 256-operation and
1 MiB payload caps. Existing buffer overflow still rejects the stage. Entity
effects remain one atomic subgroup; the staged entity owner captures spawn IDs,
but the executor attaches them to `EntitySpawned` only after both entity staging
and snapshot reservation commit. Any rejection returns no candidate IDs. Transfer
effects are still independently resolved. `Started` retains its existing
prepare-queued/readiness-pending/commit-not-completed meaning.

## Examples and source-level review

`crates/gameplay/examples/support/effect_feedback_module.rs` is the shared neutral
module used by both examples. Its Startup system emits a topology-matched spawn;
its Environment system discovers an existing world through the bounded read
view, emits a spawn on the first simulation tick, then consumes its own receipt
at the next invocation and reads the committed `EntityId` or rejection. It does
not scan world entities. The gameplay example installs the module in a neutral
registry. `crates/server-runtime/examples/embedded_host.rs` installs the same
module and drains the Startup receipt through the public host API after runtime
initialization. Both examples are compile-only; neither was executed.

| Scenario | Source-level disposition |
| --- | --- |
| Shared buffer operation or payload overflow | Existing `rejected_pushes` forces every retained effect to `StageRejected`; accepted tokens remain correlated. No stage effects are committed. |
| Producer inbox is full | `emit` returns `ReceiptBackpressure` before buffer acceptance, increments a visible diagnostic count, and does not mark the shared buffer overflowed. |
| Two spawns in one entity group | The staged owner returns IDs in input order; the executor maps them only to corresponding spawn indices after the whole subgroup and snapshot reservation commit. |
| Entity subgroup validation or storage reservation fails | Every entity effect receives typed rejection and no staged spawn ID is exposed. Transfers retain their independent result path. |
| Transfer validation or backpressure rejects one transfer | The transfer gets its own typed rejection; successful entity subgroup outcomes and other transfer outcomes remain independently correlated. |
| Unread ordinary receipts | They are visible through `take_effect_receipts` during the next invocation; any remainder is expired and counted/logged when the call returns. |
| Unread Startup receipts | They remain in the bounded host inbox. The host can drain them after initialization; shutdown returns the remainder and logs its count. A full inbox backpressures before acceptance. |
| Later world's Startup commit fails during construction | Construction returns a downcastable `StartupEffectFailure` carrying accumulated receipts and the current batch's rejected-push count, including receipts from earlier per-world commits. |
| Command handler fails after emitting | Accepted buffered effects are not committed; ordered correlated `StageRejected` receipts are attached to the existing error reply. |
| Runtime shutdown | Pending system and Startup receipts are returned in `ShutdownOutcome::effect_receipts`; receipt state is not silently discarded as successful delivery. |
| Producer sequence reaches `u64::MAX` | The maximum sequence can be accepted once; later emit returns `IdentifierSpaceExhausted` without wrapping or reusing a token. |

No protocol, save, generator, pagination, scheduler-plan, gameplay rule,
permission, inventory, or persistence format changed. Receipts are volatile and
do not promise exactly-once execution across restarts. Public Rust effect writer
constructors and `emit` return types now require/return producer tokens; all
repository consumers were migrated.

## Static checks

Commands ran in `D:\Git\minimium\.local\worktrees\e050-06` on the base SHA
above plus these worktree changes. No tests or applications/examples were run.

| Command | Status | Evidence |
| --- | --- | --- |
| `rtk cargo check -p minimium-gameplay --locked --examples` | PASS (0) | New shared feedback module and existing gameplay examples compiled. The first compile review found a stale `Ok(())` match in `neutral_command_module.rs`; it now consumes the accepted token, and the final all-examples compile passes. |
| `rtk cargo check -p minimium-server-runtime --locked --examples` | PASS (0) | Existing and updated public host examples compiled, including Startup receipt read. |
| `rtk cargo check -p minimium-gameplay -p minimium-world -p minimium-server-runtime -p minimium-game-sample -p minimium-server --locked` | PASS (0) | Affected neutral, world, runtime, sample consumer and server composition compiled. |
| `rtk cargo fmt --all -- --check` | PASS (0) | Workspace formatting clean after formatting changed files. |
| `rtk git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-06 diff --check` | PASS (0) | Final worktree diff has no whitespace errors. |
| `rtk proxy python scripts/check_architecture.py` | PASS (0) | Coordinator rerun: production dependency graph and declared feature graphs passed. |
| `rtk proxy python scripts/check_file_lengths.py` | PASS (0) | Coordinator rerun: source length check passed; all changed hand-maintained files are at or below 500 lines. |
| `rtk proxy python scripts/check_feature_contracts.py` | PASS (0) | Coordinator rerun: renderer CPU, terminal client and embedded server compile consumers passed; examples were compiled, not run. |
| Unit/integration/E2E suites, runtime scenarios, game/GPU runs | NOT RUN | Deferred by repository policy and the issue instructions. |
| `rtk proxy python scripts/check_static_refactor.py` | PASS (0) | Coordinator rerun: selected Rust/WGSL declaration locations and replaced paths passed; this is not semantic ownership proof. |

No commit, push, PR, merge, or GitHub write was performed. Post-merge main
read-back remains for the coordinator after review and acceptance.
