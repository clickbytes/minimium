# E050-01 / #522 — Neutral module commands

## Baseline and ownership

- Starting main/worktree SHA: `c3717f84c9bbdeaee78fc54b5163e0fe80926b6e` (#545 merge).
- Work branch: `work/e050-01-neutral-commands`.
- Owner: `minimium-gameplay` owns module command result, text, code validation,
  metadata and handler contracts. `minimium-server-runtime` owns the private
  handler adapter, authorization, effect commit and existing host reply format.
- Producer: game modules implement `EngineModule<ModuleCommandHandler, S>` and
  register the neutral function in the existing `CommandDefinition`.
- Consumers: `RuntimeModules::new` maps external neutral handlers into the same
  `ModuleRegistry` used by built-ins. `SampleGameplayModule` now implements the
  neutral handler type. The public example installs a real neutral module and
  registers an admin command without importing server-runtime.
- Removed path: game modules no longer implement the runtime-returning
  `ModuleCommandHandler` alias. The host-facing `ModuleCommandReply` remains the
  result type of runtime command execution; it is not the game-module handler
  contract.

## Result and preserved flow

`ModuleCommandText` retains the 8 KiB byte cap, UTF-8 boundary handling, literal
` [truncated]` marker and ignored appends after truncation, and exposes whether
truncation occurred. `ModuleCommandErrorCode::new` validates a nonempty ASCII
identifier of at most 128 bytes. Neutral success and typed failure are adapted
once to the existing runtime `CommandReply`; built-in reports continue to use
their structured payload variants.

Scope checks and resolution remain in `execute_registered`. The tick path still
commits through `commit_effect_batch` only after handler success. A handler
failure drops its buffered effects; success retains the existing per-effect
outcome and partial rejection behavior. No permissions, wire formats or
execution order changed. Protocol 21, save 11, generator 6 and independent #529
work are untouched.

## Static trace review (not runtime-tested)

| Case | Expected behavior from current code |
|---|---|
| Unknown command | Registry resolution returns none; host produces `server.command.unknown`; handler/effects are not reached. |
| Wrong scope | A resolved non-admin command produces `server.command.permission_denied` before its handler. |
| Duplicate name or alias | Registration returns `ModuleError::DuplicateCommand`; `install_mapped` stages validation and does not publish partial module state. |
| Text limit splits a UTF-8 scalar | The appended slice backs up to a character boundary, reserves marker room, appends ` [truncated]`, then ignores later appends. |
| Handler emits then returns error | The neutral error becomes a failed `CommandReply`; the tick commits only when `handler_succeeded()` is true, so the batch is discarded. |
| Handler succeeds but effect is rejected | The same commit path returns `EffectBatchOutcome`; existing response logic reports full/partial rejection as an error rather than accepted execution. |
| Module has no commands | Components/systems can be registered through the same module API; an empty command list requires no handler execution. |

## Checks

All commands used RTK from the worktree at `c3717f84c9bbdeaee78fc54b5163e0fe80926b6e`.
Cargo metadata reports the worktree-local `target` directory; subsequent scoped
checks reused those compiled artifacts.

| Command | Result |
|---|---|
| `rtk proxy cargo fmt --all -- --check` | PASS (exit 0) |
| `rtk proxy cargo check -p minimium-gameplay --locked --example neutral_command_module` | PASS (exit 0) |
| `rtk proxy cargo check -p minimium-server-runtime --locked` | PASS (exit 0) |
| `rtk proxy cargo check -p minimium-game-sample --locked` | PASS (exit 0) |
| `rtk proxy cargo check -p minimium-server --locked` | PASS (exit 0) |
| `rtk proxy python scripts/check_architecture.py` | PASS (exit 0) |
| `rtk proxy python scripts/check_file_lengths.py` | PASS (exit 0; maximum 500 lines) |
| `rtk proxy cargo tree -p minimium-gameplay --edges normal,build,dev --target all --locked` | PASS (exit 0); all-target graph has 15 distinct package nodes, including normal/build/dev edges; no `minimium-server-runtime` node. |
| `rtk proxy git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-01 diff --cached --check` | PASS (exit 0) |

Unit/integration/E2E tests, example execution, application runtime, GPU and
benchmarks were not run, per project policy.
