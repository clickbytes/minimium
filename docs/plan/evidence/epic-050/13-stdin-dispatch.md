# E050-13 — desktop developer stdin dispatch evidence

## Scope and owner

- Issue: #536, dependent on #535 CLOSED and merged.
- Base/starting HEAD: `83d29efa104237a666672d827eac395c745096f8` on `work/e050-13-stdin-dispatch`.
- Owner: `apps/client` composition/desktop adapter. The application facade continues to call the public `ClientApplicationCore` from client-runtime; this change does not alter runtime/core/session/transport/admin contracts.
- Changed behavior is limited to when the existing desktop stdin receiver reads, how typed presentation operations check their own readiness, and where host output is formatted.
- Protocol, save, generation, assets, permission rules and gameplay behavior are unchanged. Earth teleport continues through the existing client action and server permission path.

## Producer and consumer call map

| Stage | Before | After |
|---|---|---|
| Producer | `main.rs::spawn_console_input_reader` reads stdin on a worker. `sync_channel(32)` bounds queued lines; at most 512 bytes including newline are accepted. Oversized lines and full-channel sends are logged and dropped. | Unchanged. |
| Receiver | `renderer/window.rs::App::about_to_wait` reads at most one line only if a host and receiver exist **and** `scene_visible && !map_teleport_pending()`. | A host and receiver are still required; the same one `try_recv` is performed every host-loop iteration regardless of scene visibility or pending map teleport. It remains before the normal step, so session-step fairness remains. |
| Parser/executor | `renderer/dev_input::execute` → `renderer/controller::execute_input` → `ClientApplication::execute_text_command` → shared typed parser/executor → desktop action/presentation handler. | Same path and types. No widget text, history or preedit injection was added. `help` already returned typed output; `fps show` and `time status` now return their existing typed output through the common host formatter so stdin can print them. |
| Capability | Presentation operation was always executed on desktop; `Unavailable` was silently discarded by stdin formatting. | `PresentationOperation::capability` and `::requirement` classify each typed operation. `renderer/controller/presentation.rs` checks readiness before invoking its handler and returns `PresentationResult::Unavailable(PresentationUnavailable { capability, requirement, reason })` immediately. The typed `map_telepo` and `spawn_random_bolid` actions use the same readiness guard before arming/staging. No command is retained for replay. |
| Formatting | Desktop stdout ignored unavailable presentation results; terminal already formatted its own unavailable notice. | Desktop stdin prints a readable typed reason; F1 console prints that same typed message to its console. Terminal keeps its localized no-presentation notice and now constructs the same typed unavailable result. |

The developer input worker retains the 32-line bounded channel and 512-byte line bound. The shared text executor retains its existing 512-byte limit for recognized `dev ...` commands and 128-byte limit for other desktop developer-stdin commands. Each host-loop callback handles at most one line. The callback never blocks on stdin or drains the channel without a bound.

## Source-reviewed operation capability matrix

The capability and readiness are attached to each parsed `PresentationOperation` before the desktop dispatcher executes it. Host-only operations may read adapter diagnostics or local renderer controls while a scene is absent; context-sensitive data stays gated to avoid mixing retained previous-world state into a current-world operation.

| Command(s) | Required capability | Available result / blocked result |
|---|---|---|
| `dev look YAW PITCH` | Host only | Changes the local diagnostic camera direction; no scene required. |
| `dev bolid transmission on\|off` | Host only | Updates the local diagnostic toggle or prints that the bolid layer is unavailable; no world mutation. |
| `atmosphere status` | Host only | Prints current renderer status, including adapter state during loading. |
| `weather status` | Host only | Prints current weather data or its existing preparing/unavailable snapshot result. |
| `terrain status` | Host only | Prints current terrain and cache diagnostics. |
| `sky meteors status` | Host only | Prints client-local meteor settings and latest available counters. |
| `sky meteors clear` | Host only | Clears the existing local meteor list; no scene required. |
| `sky meteors debug off\|light\|occlusion\|paths\|budget` and legacy `sky meteors debug light on\|off\|transmission` | Host only | Reads or changes local diagnostic settings. |
| `dev bolid status` | Active connected world with presentation matching that world | Prints current-context retained bolid diagnostics only; missing/disconnected/pending/mismatched world returns typed unavailable. |
| `dev goto LATITUDE LONGITUDE` | Connected active Earth world, CPU sectors ready, matching presentation, no pending map teleport | Requests the existing server-authorized teleport. Missing session/world, loading, wrong world, pending teleport or stale presentation returns typed unavailable and does not queue an action. |
| `map_telepo` | Earth map teleport with `ActiveEarthWorld` readiness | Opens the overview only when connected Earth, CPU sectors and matching presentation are ready. Otherwise returns the same typed unavailable result and does not arm the map. |
| `spawn_random_bolid [N]` | Developer renderer with `ActiveWorldPresentation` readiness | Stores the existing pending gaze/admin request only for a connected active world with a matching visible render frame; otherwise typed unavailable. On acceptance, the existing queued notice uses the shared output formatter. Existing ready-to-transfer timing between queueing gaze and later admin send is not changed or re-stamped here. |
| `dev clouds` | Ready presentation matching the current context | Prints up to eight cloud aiming hints; offline demo is allowed only for its current synthetic frame, and the published cloud snapshot must match current render context and manifest. Pending/stale scene returns typed unavailable or an explicit preparing/current-world message. |
| `dev bolid path ID KIND SEED DURATION_MS X0 Y0 Z0 X1 Y1 Z1` | Active connected world with presentation matching that world | Restores the local diagnostic path for the current context; otherwise typed unavailable. |
| `dev bolid fragments` / `dev bolid lighting` | Active connected world with presentation matching that world | Prints current-frame renderer diagnostics; otherwise typed unavailable. |
| `dev bolid replay AGE [SEED [EVENT_ID]]` / `dev bolid freeze AGE [SEED [EVENT_ID]]` | Active connected world with presentation matching that world | Runs the existing local replay only for a matching current-context bolid. The operation still reports its existing typed text when no retained event or synchronized server time is available. |
| `sky info` / `sky bodies` | Ready presentation matching the current context | Prints current-frame diagnostics; when scene presentation is pending or context is stale, typed unavailable is returned immediately. Offline demo mode remains supported only while its local demo adapter is present and both frame contexts match the synthetic context derived from current instance/manifest revision. |
| `sky meteors spawn PRESET [SEED]` | Ready presentation matching the current context | Uses the existing local spawner/environment admission. No network session is required, so a ready offline demo remains supported; a pending scene returns typed unavailable. |
| Unknown or malformed `dev ...` | Shared parser result | Typed command rejection and usage/unknown-command feedback; no secondary parser or special queue. |

Ordinary host commands are independent of presentation: `help`, `quit`/`exit`, `fps show`, `net stats`, `state`, `time status`, property/config commands and `clear` continue through the shared typed executor. `quit` exits the host loop. `clear` has no stdout history to clear and does not modify the F1 console. `map_telepo` is the typed Earth-map action and uses the same current-context Earth readiness gate as `dev goto`. `spawn_random_bolid` retains its existing admin channel and server cooldown; only its local gaze/request staging is gated before it is armed.

`ActiveWorldPresentation` requires a connected `SessionState::Connected`, a nonempty session active context equal to `ClientApplicationCore`'s current context, a visible scene, and an environment render-frame context equal to that same core context. `ActiveEarthWorld` adds Earth identity, CPU sectors readiness and the presentation check. The map-teleport pending guard applies before both. This prevents an operation from using a prior render frame or a new world before the active context changes. A server Prepare keeps the source context authoritative until Commit; source-context operations before Commit still use that source. Once Commit changes the context, a stale frame fails the equality check until its matching presentation is installed. No request is moved to or replayed against a later world.

`ReadyPresentation` accepts connected sessions only when active session context, core context and render-frame context are all the same nonempty value. It never treats `None == None` as proof of readiness. Its offline exception is restricted to a live `LocalDemoEnvironment` with no session and no busy connection attempt, no core context, LocalDemo clock quality, current instance/manifest-derived synthetic context, and matching current input/render descriptors and contexts. Starting a connection clears the demo adapter; stale frames after disconnect cannot pass this predicate.

## Edge cases reviewed

| Edge | Source behavior and expected result |
|---|---|
| Host not initialized yet | No `DesktopHost` exists, so the event loop cannot consume stdin. The worker can queue at most 32 lines; later full sends log `client_runtime.console_input_queue_full` and drop the new line. After host initialization, the normal one-line receive begins. |
| Host exists, Earth/world loading | Input is received. Host-only help/status/quit works. `dev goto` requires connected active Earth, CPU readiness and matching presentation; it returns the first typed unavailable reason and sends no teleport. `map_telepo` returns that same typed reason and does not arm the overview. `spawn_random_bolid` returns typed unavailable before storing a gaze/admin request. Scene-bound commands return pending/readiness reason and are not retried. |
| Scene GPU presentation absent | `ReadyPresentation` returns `PresentationPending`, or `CpuReadinessPending` while `ClientApplication::is_loading()` is true. Render context mismatch returns `WorldTransferPending`. |
| Pending map teleport | The line is still received. Context-sensitive operations return `WorldTransferPending`; nothing is queued for after teleport. Host-only status/help/quit remain usable. |
| Server Prepare / portal transition | Source remains active until Commit. Operations may use only source context while it is still the active core/session/render context. After Commit, frame/context mismatch blocks world-sensitive operations until the destination frame matches. |
| Disconnected session | Session-sensitive operations require `SessionState::Connected` and an active matching context and return typed unavailable otherwise. Host-only diagnostics/local toggles remain callable even if the old `scene_visible` flag is still true. |
| Offline demo | Presentation-only local meteor spawn, cloud hints and sky diagnostics are allowed only when the local demo adapter is still live and its current synthetic input/render contexts match. Starting a connection removes that adapter; a retained prior frame after disconnect cannot pass the check. Network-context operations such as Earth teleport and retained server bolid inspection remain unavailable without a connected active world. |
| Unsupported presentation command in terminal | The terminal builds `PresentationResult::Unavailable` with `UnsupportedHost`, then uses its existing localized no-presentation message. |
| Unknown/invalid developer operation | The shared parser returns the normal typed rejection/usage output; no operation handler runs. |
| Oversized line | The producer discards a line above 512 bytes including newline and emits `client_runtime.console_input_too_long`; no partial line reaches parsing. |
| Full channel | The existing bounded `try_send` logs `client_runtime.console_input_queue_full` and drops the incoming line. It never blocks the stdin worker. |
| Closed receiver | `try_recv` yields an error, the callback does no command work and continues the host/session step. No blocking receive or retry queue is introduced. |
| No queued line | `try_recv` returns immediately; the host continues its normal 16 ms step schedule. |

## Changed files and checks

Changed files:

- `apps/client/src/renderer/window.rs`
- `apps/client/src/renderer/dev_input.rs`
- `apps/client/src/client_commands/presentation.rs`
- `apps/client/src/client_commands.rs`
- `apps/client/src/renderer/controller/presentation.rs`
- `apps/client/src/renderer/clouds/runtime.rs`
- `apps/client/src/renderer/controller/action.rs`
- `apps/client/src/renderer/controller/output.rs`
- `apps/client/src/terminal_client/actions.rs`
- `apps/client/src/terminal_client.rs`
- `apps/client/src/client_commands/help.rs`
- `apps/client/README.md`
- `docs/commands.md`
- `docs/plan/evidence/epic-050/13-stdin-dispatch.md`

## Static verification

Working directory for all commands: `D:/Git/minimium/.local/worktrees/e050-13`. The worktree remains on `work/e050-13-stdin-dispatch`, based at `83d29efa104237a666672d827eac395c745096f8`; implementation is uncommitted for root review/integration.

| Command | Status | Result |
|---|---|---|
| `rtk cargo fmt --all -- --check` | PASS | Exit 0. |
| `rtk cargo check -p minimium-client --locked` | PASS | Exit 0. |
| `rtk cargo check -p minimium-client --no-default-features --locked` | PASS | Exit 0, no warnings after feature-gating desktop-only unavailable reasons. |
| `rtk proxy python scripts/check_static_refactor.py` | PASS | Exit 0; selected Rust/WGSL declarations and replaced paths pass. This does not prove semantic ownership. |
| `rtk git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-13 diff --check` | PASS | Exit 0. |
| `rtk proxy python scripts/check_architecture.py` | PASS | Exit 0, run by root on this worktree/base. Four declared graphs report 37/73/46/49 packages; production/target/build are checked with the dev-only edge separate. |
| `rtk proxy python scripts/check_file_lengths.py` | PASS | Exit 0, run by root on this worktree/base; 808 hand-maintained sources, maximum 500 lines. |
| Unit/integration/E2E tests, game, GPU, runtime acceptance and benchmarks | NOT RUN | Deferred by the repository policy and E050 scope; this change was not functionally exercised. |

## Remaining runtime verification

No interactive application, GPU session, stdin timing, command permission or transfer runtime was started. The static checks establish compilation and source-level call paths only. Q03 retains live acceptance for loading, disconnect, map teleport and scene transitions. At implementation handoff, the task owner had performed no commit, push, PR creation or GitHub write; integration belongs to the coordinator.

## Final source review

Coordinator and independent reviewer: PASS on the complete adapter diff, including the final `map_telepo` and `spawn_random_bolid` guards. Review corrections cover connected/demo context separation, live offline-demo admission, cloud publication identity and current-context bolid diagnostics. The one-line receive bound and shared parser remain unchanged. The existing accepted-request gaze delay and its ready-world-to-transfer timing risk remain explicitly deferred above. This source review does not claim runtime acceptance.
