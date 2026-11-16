# E050-02 / #524 — Public typed host replies

## Baseline and ownership

- Issue: [#524](https://github.com/clickbytes/minimium/issues/524); epic:
  [#520](https://github.com/clickbytes/minimium/issues/520).
- Predecessor #522 was CLOSED (completed). Local `main` contains merge #546 at
  `99e09896ce5f6373efb4a5ef58ec8f274608f512` before work began.
- Branch/worktree: `work/e050-02-typed-replies`,
  `D:/Git/minimium/.local/worktrees/e050-02`.
- Owner: `minimium-server-runtime` owns the reply's authoritative private
  payload/failure/effect state and now supplies an immutable public borrowed
  projection. The application host owns submission/step; terminal and admin
  remain formatting adapters.
- Producer: existing command handlers in `commands.rs`, `commands/world.rs`,
  `runtime/tick/properties.rs`, `runtime/bolid.rs`, impact spawn and the neutral
  module adapter.
- Consumers: `CommandExecution.reply.view()` for embedded hosts;
  `command_output.rs` for shared rendering; `admin.rs` for admin success/error
  envelope. `typed_replies.rs` demonstrates an external library consumer using
  the existing embedded composition and ordinary `step`.

## Public projection and variant map

`CommandReply::view()` returns `CommandReplyView` with `ReplyState`, independent
`should_stop`, bounded `error_code`/`detail`, an optional exhaustive payload
view and an immutable effect-outcome reference. It does not expose mutation
handles or the private result enum. Report and row structures use private fields
with read-only getters; variable lists are borrowed slices and retain their
existing truncation flags. Runtime metrics, clock snapshot, impact status,
excavation status, help and property details are publicly nameable and readable.
Module text exposes both bounded text and its truncation flag.

| Existing private `CommandPayload` | Public `CommandPayloadView` |
|---|---|
| `Help` | `Help` (built-ins, module help rows, truncation) |
| `Status` | `Status` (all status counters, persistence and optional runtime report) |
| `Players` | `Players` (bounded rows, truncation) |
| `Metrics` | `Metrics` (all metric fields, client traffic rows, truncation) |
| `SaveQueued` | `SaveQueued` |
| `Stopping` | `Stopping` |
| `Time` | `Time` (clock identity, nonce, mode, scale, UTC, revision and fault data) |
| `Property` | `Property` (requested/effective/default/source/reason) |
| `UnknownProperty` | `UnknownProperty` |
| `UnavailableProperty` | `UnavailableProperty` |
| `PropertyHelp` | `PropertyHelp` |
| `PropertyCommandsHelp` | `PropertyCommandsHelp` |
| `SingleCommandHelp` | `SingleCommandHelp` |
| `PropertyList` | `PropertyList` (prefix, rows, truncation) |
| `WorldList` | `WorldList` (rows, truncation) |
| `WorldInspect` | `WorldInspect` (identity, players, full manifest) |
| `EntityList` | `EntityList` (rows and entity/player identity, truncation) |
| `EntityInspect` | `EntityInspect` (world/entity, component type/schema/bytes, truncation) |
| `EntityDespawn` | `EntityDespawn` |
| `Bolid` | `Bolid` |
| `ImpactSpawn` | `ImpactSpawn` |
| `ModuleText` | `ModuleText` (text and truncation) |

The shared formatter and admin adapter consume the same view. No string parsing
was added. A handler failure exposes its bounded code/detail without a success
payload. Effect rejection keeps the already-produced payload readable and the
typed ordered `EffectBatchOutcome` available; full rejection is `Rejected`,
mixed applied/rejected effects are `Partial`. Stop is independent of reply
state, so a stop request remains distinguishable. `SaveQueued` means queue
acceptance only; the existing `StatusReport::persistence()` is the observation
of pending/last-save completion. Existing list/byte caps and formatter
truncation markers are retained.

## Compatibility and static negative review

- No `RuntimeCommand` grammar/input redesign or pagination, feedback, permission,
  scheduler, or wire changes. The example uses the existing parser and does not
  create a second runtime.
- Protocol v21, save v11, generator v6, permission behavior, admin wire envelope,
  effect commit order and existing output wording remain unchanged.
- Handler error without payload remains typed rejection; error detail keeps the
  existing bounded-text behavior and code limit.
- Success with rejected effects retains payload and typed effect outcomes;
  overall status cannot be mistaken for full success.
- Bounded/truncated help, player/world/entity/property/metric rows remain slices
  of the owner-generated report. Module UTF-8 text keeps its existing byte cap
  and truncation marker, with a separate public truncation indicator.
- A queued save remains non-durable. Completion/failure continues to come only
  from existing persistence status; no second completion state was added.
- Shutdown cancellation remains an execution failure from the existing runtime
  path. Stop is read from the reply disposition and does not imply durability.

## Checks

Commands ran from the worktree at
`99e09896ce5f6373efb4a5ef58ec8f274608f512`, with the repository RTK wrapper.
The final staged diff was checked after staging all files listed below.
An initial workspace format check during example review exited 1 and reported
an import-order diff in the `#[path]` support module. A subsequent direct check
showed the shared support file already matches rustfmt; no support-file change
is retained. The final workspace format check passes.

| Command | Result |
|---|---|
| `rtk git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-02 diff --check` | PASS (exit 0) |
| `rtk cargo fmt --all -- --check` | PASS (exit 0) |
| `rtk proxy rustfmt --check --config skip_children=true --edition 2024 crates/server-runtime/examples/typed_replies.rs` | PASS (exit 0; checks the example independently of its `#[path]` support module) |
| `rtk cargo check -p minimium-server-runtime --locked --example typed_replies` | PASS (exit 0) |
| `rtk cargo check -p minimium-server-runtime --locked --example embedded_host` | PASS (exit 0) |
| `rtk cargo check -p minimium-server -p minimiumctl --locked` | PASS (exit 0) |
| `rtk python scripts/check_architecture.py` | PASS (exit 0) |
| `rtk python scripts/check_file_lengths.py` | PASS (exit 0; all files at or below 500 lines) |
| `rtk git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-02 diff --cached --check` | PASS (exit 0; staged patch including new files) |
| Unit/integration/E2E suites, example execution, server/GPU runtime, benchmarks | NOT RUN per project policy |

No runtime/visual behavior is claimed as verified. No commit, push or GitHub
write was performed.

## Changed files

- `architecture.toml`
- `crates/server-runtime/README.md`
- `crates/server-runtime/examples/typed_replies.rs`
- `crates/server-runtime/src/admin.rs`
- `crates/server-runtime/src/command_output.rs`
- `crates/server-runtime/src/commands.rs`
- `crates/server-runtime/src/commands/reply_data.rs`
- `crates/server-runtime/src/environment/clock.rs`
- `crates/server-runtime/src/environment/mod.rs`
- `crates/server-runtime/src/hosting.rs`
- `crates/server-runtime/src/hosting/replies.rs`
- `crates/server-runtime/src/lib.rs`
- `crates/server-runtime/src/metrics.rs`
- `crates/server-runtime/src/runtime.rs`
- `crates/server-runtime/src/runtime/excavation.rs`
- `crates/server-runtime/src/runtime/excavation/metrics.rs`
- `crates/server-runtime/src/runtime/impacts.rs`
- `docs/plan/evidence/epic-050/02-typed-replies.md`
