# E050-03 / #525 — Typed host command input

## Baseline and owner

- Issue: [#525](https://github.com/clickbytes/minimium/issues/525); epic:
  [#520](https://github.com/clickbytes/minimium/issues/520).
- Predecessor: #524 was verified CLOSED; the matching #547 merge is the current
  `main` base `bb35cc1cb9893ac81c957d123890eec0550d4d99`.
- Starting source SHA: `bb35cc1cb9893ac81c957d123890eec0550d4d99`.
- Branch/worktree: `work/e050-03-typed-input`,
  `D:/Git/minimium/.local/worktrees/e050-03`.
- No commit or push was created. All listed changes are staged for review.
- Owner: `minimium-server-runtime` owns validated `RuntimeCommand` constructors,
  parsing into private intents, shared bounded admission and owner-lane execution.
  `minimium-environment` continues to own clock value types and parsers;
  `minimium-commands` continues to own property request syntax; module
  registration and handlers continue to own command identity/semantics.

## Producer → intent → executor

Before, embedded hosts had to format a command line and call
`RuntimeCommand::parse`; console and admin text adapters parsed to private
`ServerCommand`. All three then reached `enqueue_command` and the same tick
executor. Registered modules received a whole line and resolved it with a lexer
inside the registry.

After, typed constructors create the same private `ServerCommand` intents
without text formatting or parsing. `submit` still enqueues that intent in the
same bounded host queue. Console/admin text parsing still produces the same
intent and uses the same queue. Module text is lexed once to argv; the runtime
matches the longest registered canonical name/alias and passes the remaining
argv slice. Typed module commands carry an exact registered identity separately
from a bounded argument vector and use exact identity matching. Both paths reach
the same scope check, handler, owner effect commit and reply.

| Existing `ServerCommand` operation | Typed host construction |
|---|---|
| `Help` | `RuntimeCommand::help()` |
| `Status` | `RuntimeCommand::status()` |
| `Players` | `RuntimeCommand::players()` |
| `Metrics` | `RuntimeCommand::metrics()` |
| `Save` | `RuntimeCommand::save_world()` |
| `Stop` | `RuntimeCommand::stop()` |
| `SpawnRandomBolid(count)` | `spawn_random_bolids(count)`; shared 1..=64 validator |
| `SpawnRandomMeteorid` | `spawn_random_meteorite()` |
| `Time(TimeCommand)` | `time(TimeCommand)`; owner clock types plus shared minimum-scale validation |
| Property `Get` | `get_property(key)` |
| Property `SetAutosave` | `set_autosave_interval(seconds)`; shared 0..=604800 validator |
| Property `ResetAutosave` | `reset_autosave_interval()` |
| Property `SetBolidInterval` | `set_bolid_interval(Option<(min,max)>)`; shared 10..=3600 ascending-range validator |
| Property `ResetBolidInterval` | `reset_bolid_interval()` |
| Property `List` | `list_properties(prefix)` |
| Property `Help` | `property_help(target)` |
| Registered module command | `module(identity, argv)`; identity remains separate from bounded argv |

Typed setting values do not pass through `format!`/`to_string` and parsing.
Text property parsing and typed numeric setters use the same range helpers.
Text and typed clock commands use the same validation for clock type bounds and
the minimum nonzero rate; their error wrappers retain the text parser's current
codes and details. Module-specific arguments remain argv strings, as allowed by
the issue contract; quoted text arguments retain their existing lexical
boundaries. Module command aliases remain case-sensitive and use longest-prefix
matching for text input.

The submission path still checks `Stopping`, queue capacity 64 and identifier
exhaustion before admission. Tick execution remains capped at eight commands;
execution results and cancellation outcomes remain separate from enqueue
acceptance. Admin text uses the server-admin adapter and remote principals do
not receive a host `RuntimeCommand` path. No permission, persistence, wire,
protocol, save, generator, clock scheduling or tick-order changes are intended.

## Static negative review (not executed scenarios)

- Out-of-range bolid count, autosave interval and bolid interval are rejected by
  shared typed/text validators. Entity/world module argument parsing remains in
  the registered module handler and therefore retains its existing diagnostics.
- `TimeScalePpm` prevents NaN/infinity; the runtime constructor also rejects
  nonzero scales below the text parser's 0.1x minimum. `WorldTimeMs`' supported
  range equals the RFC3339 parser range (2000-01-01 inclusive to 2101-01-01
  exclusive).
- Oversized property fields and module identity/argv are rejected at typed
  construction. A module handler error still discards its emitted effect batch;
  scope denial still occurs before invoking the handler.
- Queue full/stopping/ID exhaustion behavior is unchanged in `submit`; those
  conditions, permissions, cancellation and runtime outcomes were not run.
- No pagination code, new executor, new parser grammar, or unbounded string
  execution path was added.

## Static checks

Commands ran from the isolated worktree based on
`bb35cc1cb9893ac81c957d123890eec0550d4d99`, using the required RTK wrapper.
The complete staged patch, including new files and this evidence, was checked.

| Exact command | Result |
|---|---|
| `rtk git diff --check` | PASS (exit 0; no unstaged whitespace errors) |
| `rtk git diff --cached --check` | PASS (exit 0; complete staged patch) |
| `rtk cargo fmt --all -- --check` | PASS (exit 0) |
| `rtk cargo check -p minimium-server-runtime --locked --example typed_host_commands` | PASS (exit 0) |
| `rtk cargo check -p minimium-server-runtime --locked --examples` | PASS (exit 0) |
| `rtk cargo check -p minimium-server -p minimiumctl --locked` | PASS (exit 0) |
| `rtk python scripts/check_architecture.py` | PASS (exit 0; declared feature graphs and production dependencies) |
| `rtk python scripts/check_file_lengths.py` | PASS (exit 0; 769 source files, maximum 500 physical lines) |
| Unit/integration/E2E suites, execution of examples, server/game/GPU runtime, benchmarks | NOT RUN (deferred by project policy and issue scope) |

No runtime or visual acceptance is claimed. Q03 remains the runtime acceptance
owner. No GitHub write, commit or push was performed.

## Changed files

- `architecture.toml`
- `crates/server-runtime/README.md`
- `crates/server-runtime/examples/embedded_host.rs`
- `crates/server-runtime/examples/typed_host_commands.rs`
- `crates/server-runtime/src/commands.rs`
- `crates/server-runtime/src/hosting/commands.rs`
- `crates/server-runtime/src/runtime/tick.rs`
- `crates/server-runtime/src/runtime/tick/command_execution.rs`
- `crates/server-runtime/src/server_command.rs`
- `crates/server-runtime/src/server_command/property_parsing.rs`
- `docs/plan/evidence/epic-050/03-typed-input.md`
