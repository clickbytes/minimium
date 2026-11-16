# E050-12: Public CPU client application core

Base/main: `d0f997a84a1b73e87c8ab630b28f47377b37eff6`.
Branch/worktree: `work/e050-12-public-client-core`, `.local/worktrees/e050-12`.
Predecessor #534 was read CLOSED after PR #557 merged, main SHA/tree read-back
and CI `43938688120` SUCCESS. Review covered the uncommitted migration on this
base; the resulting commit and merged main SHA are recorded in #535 after
integration. This document describes source/static acceptance, not runtime PASS.

| Responsibility | Before | After / actual consumers |
| --- | --- | --- |
| Session, world/catalog and transfer facts | Private app ClientApplication | One runtime ClientApplicationCore; desktop facade, terminal and public CPU host consume typed operations/views. |
| Actions, prediction and reconciliation | App-only executor and desktop-gated full step | Runtime public typed actions/full CPU step with the same integrator, queue semantics and time/readiness contracts. |
| Input suppression | Reads of console/overview inside app step | Explicit host policy in PredictionInput; desktop supplies prior rules, terminal supplies no prediction. |
| UI and GPU staging | App mixed with logical owner | App facade retains console/input/settings/status/overview and derived immutable presentation stages. |
| Transport, jobs, source and readiness | Existing stages 07-11 | Same ports/providers; core composes them, including an explicitly adopted prepared session in the existing controller. |

Protocol 21, save 11, generator 6, bytes/IDs, source fingerprints, physics and
cooldown constants remain unchanged. No crate, solver or executor was added.

## Ownership and behavior

`minimium-client-runtime::ClientApplicationCore` is the single owner of the
logical client session, active world/catalog, prediction, reconciliation,
typed actions, transfer lifecycle and CPU readiness identity. Its public
`step` is the only application session-message dispatch path. Hosts read an
immutable `ClientView`, supply host-prepared maps/tool policy/bootstrap terrain,
and use explicit `ClientTime` for state-changing entrypoints. Authoritative
altitude, health, teleport and snapshot updates remain inside session dispatch.

The app facade keeps settings, console/input, desktop presentation transitions,
renderer staging and host readiness adapters. It has no second game/session
owner and exposes no mutable core/controller/game reference. Desktop input
suppression is explicit for console/overview. Terminal omits prediction and
presentation. Disconnect clears stale bootstrap terrain; the app reprovisions
current offline terrain only after the session is released, while cold Earth
content remains unavailable until supplied. CPU collision readiness remains a
separate prerequisite from optional presentation readiness.

Session adoption installs an AwaitingWelcome session into the same controller,
rejects a busy/advanced session and foreign/reused content service identity,
reserves future connector IDs, and retains logger/schema/service configuration.
Adoption never marks an external session as owning a local server; any guard
already owned by the supplied controller remains with that same controller.
Typed host readiness/material handoffs preserve time and provider failures.

`cpu_client` composes the bounded local transport and content-job ports, sends
Welcome, completes prepared world content, dispatches initial ImpactSync and a
full empty excavation batch, steps nonzero movement through CPU collision
readiness, sends an acknowledged Snapshot and inspects the immutable reconciled
view. It is compile-only and was not executed.

Source review caught and corrected an unsupported generator revision (`1`) in
the example manifest and the existing unknown-generator test fixture; both now
use supported revision `6`, matching the neighboring CPU/content examples. All
new or touched manifest constructors were checked against the current generator
contract.

## Exact changed paths

- `Cargo.lock`
- `architecture.toml`
- `docs/architecture.md`
- `docs/plan/evidence/epic-050/12-public-client-core.md`
- `scripts/check_static_refactor.py`
- `apps/client/AGENTS.md`
- `apps/client/README.md`
- `apps/client/src/application.rs`
- `apps/client/src/application/actions.rs` (deleted)
- `apps/client/src/application/desktop_ops.rs`
- `apps/client/src/application/desktop_view.rs`
- `apps/client/src/application/game.rs` (deleted)
- `apps/client/src/application/messages.rs` (deleted)
- `apps/client/src/application/step.rs` (deleted)
- `apps/client/src/application/step/readiness.rs` (deleted)
- `apps/client/src/application/time.rs` (deleted)
- `apps/client/src/application/transitions.rs`
- `apps/client/src/application/world_readiness.rs` (deleted)
- `apps/client/src/client_commands.rs`
- `apps/client/src/client_commands/movement.rs`
- `apps/client/src/client_commands/text.rs`
- `apps/client/src/renderer.rs`
- `apps/client/src/renderer/bolids.rs`
- `apps/client/src/renderer/camera.rs`
- `apps/client/src/renderer/clouds/runtime.rs`
- `apps/client/src/renderer/controller.rs`
- `apps/client/src/renderer/controller/action.rs`
- `apps/client/src/renderer/dev_input.rs`
- `apps/client/src/renderer/environment/clock.rs`
- `apps/client/src/renderer/environment/runtime.rs`
- `apps/client/src/renderer/excavation.rs`
- `apps/client/src/renderer/excavation_debug.rs`
- `apps/client/src/renderer/gameplay_presentation.rs`
- `apps/client/src/renderer/init.rs`
- `apps/client/src/renderer/materials.rs`
- `apps/client/src/renderer/overview.rs`
- `apps/client/src/renderer/session.rs`
- `apps/client/src/renderer/session/message_handling.rs`
- `apps/client/src/renderer/session/message_handling/gameplay.rs`
- `apps/client/src/renderer/session/remote_players.rs`
- `apps/client/src/renderer/session/sector_readiness.rs`
- `apps/client/src/renderer/session/world_install.rs`
- `apps/client/src/renderer/shadow_prepare.rs`
- `apps/client/src/renderer/terrain_status.rs`
- `apps/client/src/renderer/window.rs`
- `apps/client/src/terminal_client.rs`
- `crates/client-runtime/AGENTS.md`
- `crates/client-runtime/Cargo.toml`
- `crates/client-runtime/README.md`
- `crates/client-runtime/examples/cpu_client.rs`
- `crates/client-runtime/src/application/actions.rs`
- `crates/client-runtime/src/application/game.rs`
- `crates/client-runtime/src/application/messages.rs`
- `crates/client-runtime/src/application/mod.rs`
- `crates/client-runtime/src/application/ops.rs`
- `crates/client-runtime/src/application/readiness.rs`
- `crates/client-runtime/src/application/step.rs`
- `crates/client-runtime/src/application/time.rs`
- `crates/client-runtime/src/application/world_readiness.rs`
- `crates/client-runtime/src/connection_controller.rs`
- `crates/client-runtime/src/connection_controller/prepared_session.rs`
- `crates/client-runtime/src/connector.rs`
- `crates/client-runtime/src/content_jobs.rs`
- `crates/client-runtime/src/lib.rs`
- `crates/client-runtime/src/session.rs`
- `crates/client-runtime/src/session/tests.rs`
- `crates/client-runtime/src/view.rs`

## Static checks

All PASS entries were observed with exit code 0. Working directory:
`D:/Git/minimium/.local/worktrees/e050-12`. Existing fixture code was adapted
without adding or executing tests; test-target compilation remains NOT RUN.

- `rtk cargo check -p minimium-client-runtime --example cpu_client --locked` — PASS.
- `rtk cargo check -p minimium-client --locked` — PASS.
- `rtk cargo check -p minimium-client --no-default-features --locked` — PASS.
- `rtk cargo fmt --all -- --check` — PASS after formatting.
- `rtk python scripts/check_file_lengths.py` — PASS, all 808 source files within 500 physical lines.
- `rtk python scripts/check_architecture.py` — PASS, all-feature production graph and architecture checks.
- `rtk proxy python scripts/check_agent_contracts.py` — PASS, 7 AGENTS files, 7 named Skills and 23 local links.
- `rtk proxy python scripts/check_static_refactor.py` — PASS after moving the `ClientGameState` declaration-location contract to `crates/client-runtime/src/application/game.rs`.
- `rtk proxy python scripts/check_feature_contracts.py` — PASS; all four declared graphs and compile consumers passed.
- `rtk cargo tree -p minimium-client-runtime --no-default-features --edges normal,build,dev --target all --prefix none --format "{p}"` — PASS; selected all-target graph contains no `minimium-game-sample`, `minimium-astronomy`, renderer/client, GPU or window dependency.
- Tests, suites, application/example execution, GPU, network and benchmarks — NOT RUN, as required.

Runtime/visual acceptance remains for the project’s final verification phase.

## Independent coordinator verification

After the revision-6 correction, the coordinator independently ran:

| Exact command | Result |
| --- | --- |
| `rtk proxy cargo check --workspace --locked` | PASS, exit 0. |
| `rtk proxy cargo check -p minimium-client-runtime --locked --examples` | PASS, exit 0; includes earlier consumers 08-11 and cpu_client. |
| `rtk proxy cargo check -p minimium-client-runtime --locked --example cpu_client` | PASS, exit 0. |
| `rtk proxy cargo check -p minimium-client --locked` | PASS, exit 0; no production warning. |
| `rtk proxy cargo check -p minimium-client --no-default-features --locked` | PASS, exit 0; no production warning. |
| `rtk proxy cargo fmt --all -- --check` | PASS, exit 0. |
| `rtk proxy python scripts/check_file_lengths.py` | PASS, exit 0; 808 files, max500. |
| `rtk proxy python scripts/check_architecture.py` | PASS, exit 0; graphs 37/73/46/49; one separate development edge. |
| `rtk proxy python scripts/check_agent_contracts.py` | PASS, exit 0; 7 AGENTS, 7 Skills, 23 links. |
| `rtk proxy python scripts/check_static_refactor.py` | PASS, exit 0; only the moved declaration path was adjusted. |
| `rtk proxy python scripts/check_feature_contracts.py` | PASS, exit 0; four real compile consumers, never executed. |
| `rtk proxy cargo tree -p minimium-client-runtime --locked --no-default-features --edges normal,build,dev --target all --prefix none --format "{p}"` | PASS, exit 0; 105 output lines inspected, no sample/astronomy/renderer/client/GPU/window package. |
| `rtk proxy git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-12 diff --check` | PASS, exit 0 after removing a trailing blank line from Cargo.toml. |
| `rtk proxy git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-12 diff --cached --check` | PASS, exit 0. |

The canonical staged `--no-renames --name-only` list contains exactly the 67
paths above (60 Git files after rename detection), with no missing or extra path.

The earlier diff check failed on that trailing blank line; the corrected command
passed. An initial coordinator tree invocation used an unquoted PowerShell format
argument and failed; the quoted command above ran successfully. Shared example
helper warnings remain visible: host_clock 7, injected_transport 4 and
injected_content_jobs 1. No broad dead-code allowance was added.

Source review also checked constructor idle/cancelled-worker exclusion and
existing/adopted session state, service Arc/ID reservation for reconnect, typed
time errors, current-world bootstrap reprovision, current/candidate coverage,
queue-full result propagation, UI suppression, transfer prepare/cancel/commit, local admin
permissions and shutdown. It corrected MapTeleport pending admission, stale UI
arm/loading flags on close/disconnect, and the distinction between reconciler
player count and the interpolated display buffer. There is one logical owner;
old app implementation files are deleted and all known consumers are connected.

No tests, examples/applications, network sessions, GPU/visual workloads or
benchmarks were executed. Live scheduling, cancellation latency, camera/input
behavior, transfer races, physical movement/reconciliation and visual continuity
remain explicit final-phase verification gaps.
