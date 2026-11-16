# E050-05 world discovery evidence

## Provenance

- Repository: `clickbytes/minimium`
- Issue: [#527 E050-05](https://github.com/clickbytes/minimium/issues/527)
- Prerequisite: #526 closed; [PR #549 merged](https://github.com/clickbytes/minimium/pull/549)
- Base / current `HEAD`: `2a0b91a768b200b07883eab38807de1b6c20cc91`
- Worktree branch: `work/e050-05-discovery`

## Owner and call path

`minimium-gameplay` owns the neutral `GameplaySystemContext::worlds` method and
returns the shared `CommandViewPage<CommandWorldView>` / `CommandViewError`
types. The caller supplies `CommandViewRequest`, so the existing 256-row and
1 MiB caps, keyset cursor validation, membership revision, and typed stale
cursor behavior remain owned by the existing read model.

At each stage invocation, `ServerSchedule::run_stage` borrows a
`RuntimeCommandReadView` over the current `WorldRegistry` and passes that same
reader to `ModuleSystemContext`. The adapter delegates directly to its existing
`ModuleCommandReadView::worlds` implementation. Cursor identity comes from the
registry's existing read identity. No second sorter, cursor format, snapshot,
world registry exposure, or sample-world dependency was added.

`ModuleSystemContext` remains the only `GameplaySystemContext` implementation
in the repository. Portal Startup still uses its per-world context, while the
interaction handler still uses the world ID from its event. Startup dispatch and regular stage call
counts/order are unchanged; regular stages still run once with no current-world
instance and may use discovery independently.

## Example and bounded work

`crates/gameplay/examples/world_discovery.rs` registers a normal `Environment`
system with the same `ModuleCommandHandler` generic used by server composition.
Each invocation requests at most four pages, eight rows and 64 KiB per page,
follows returned cursors, and uses each discovered ID for the optional
`terrain_read` lookup. The sample therefore processes up to 32 world rows per
invocation; a larger registry requires a consumer to retain a cursor value for
later invocations. It handles `StaleCursor` by ending the current pass and
handles an empty registry by making no terrain reads. Its ServerOnly marker
component is registered with its own `ComponentTypeId`; it has no fixed world
IDs and introduces no `minimium-server-runtime` dependency.

## Static scenario review (not executed)

| Scenario | Expected code path/result |
| --- | --- |
| Zero worlds | The worlds page is empty with `next=None`; the example performs no terrain read. |
| World membership changes between pages | The existing reader rejects a changed membership revision as `StaleCursor`; the example stops that pass. The current owner has no runtime world-removal API, so removal is not a reachable scenario today. |
| Cursor from another query/runtime | Existing cursor validation rejects the mismatch as `StaleCursor`; no empty-page success is fabricated. |
| World exists but terrain is unavailable | `terrain_read(world.id)` returns `None`; discovery remains independent of terrain availability. |
| Restored Startup | Existing per-world Startup lifecycle and initialization path is unchanged. The sample runs at Environment. |
| Normal phase with `world_instance=None` | World discovery reads the registry-wide page service and does not require the optional current-world context. |
| More than the sample's page budget | The sample stops after four bounded requests; consumers can retain the returned cursor value and continue on a later call. |

## Static checks

All commands ran in `D:\Git\minimium\.local\worktrees\e050-05` against the
base `HEAD` above plus the staged worktree changes. Exit code is shown explicitly.

| Command | Status | Evidence |
| --- | --- | --- |
| `rtk cargo check -p minimium-gameplay --locked --example world_discovery` | PASS (0) | New neutral example compiled. |
| `rtk cargo check -p minimium-gameplay -p minimium-server-runtime -p minimium-game-sample -p minimium-server --locked` | PASS (0) | Relevant owner, runtime, sample consumer and server composition compiled. |
| `rtk cargo check -p minimium-gameplay --locked --example neutral_command_module` | PASS (0) | Existing gameplay example compiled. |
| `rtk cargo check -p minimium-server-runtime --locked --example paged_reads --example typed_host_commands --example typed_replies` | PASS (0) | Existing runtime examples compiled. |
| `rtk cargo fmt --all -- --check` | PASS (0) | Formatting check clean. |
| `rtk git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-05 diff --cached --check` | PASS (0) | Staged diff has no whitespace errors. |
| `rtk python scripts/check_architecture.py` | PASS (0) | Production dependency graph and all declared feature graphs passed. |
| `rtk python scripts/check_file_lengths.py` | PASS (0) | 773 source files checked; 500-line limit passed. |

No tests or example binaries were run. No server process, GPU session, or runtime
scenario was executed. These remain deferred by repository policy; the scenario
table is source-level reasoning, not runtime evidence.
