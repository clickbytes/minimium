# Client runtime instructions

Additional instructions for `crates/client-runtime/`. Root `AGENTS.md`, `crates/AGENTS.md`, `architecture.toml` and this package's `README.md` remain authoritative; do not duplicate their general rules.

## Scope and owners

- This runtime owns nonblocking protocol sessions, connection controller/connector, process-owned local-server guard, bounded content preparation, the public CPU application core, client-side prediction/reconciliation and private identity-bound collision snapshots/readiness.
- The host application owns its window, console, local input, GPU resources, render geometry and world-content resolver implementation. No `wgpu`, `winit`, renderer, application or sample-game dependency may be introduced here.
- Network/server snapshots are authoritative. Prediction is provisional and must reconcile through the shared simulation integrator; presentation offsets do not change simulated position.
- A queue result proves only local buffer acceptance, not that the server accepted the gameplay action.

## Session and world transitions

- Model connection attempts, cancellation, busy/retry and disconnection as explicit state transitions. The controller must validate its public preconditions, even when a frontend already disables an action.
- Preserve bounded wire buffers, heartbeats, content tickets, deadlines, fingerprints, manifest checks, schema and typed error causes.
- Carry `WorldContext`, instance, content identity and preparation ticket through asynchronous results. Reject completions from a stale context/ticket before publication.
- Prepared content for a destination stays **pending** until the corresponding `Prepare/Ready/Commit` handshake finishes; `Cancel/Fail/Disconnect` removes pending work without corrupting active content. Do not replace the active geography with a pending-world value.
- Desktop and terminal may have different readiness adapters, but must complete the same protocol lifecycle. No session state may rely on a console widget being present.
- `ClientApplicationCore::step` is the sole public application step and session-message application path. Keep message application crate-private; hosts consume immutable `ClientView` values and use typed actions/readiness reports with explicit `ClientTime`.

## Time and movement

- Use real monotonic elapsed time for connection, heartbeat, deadline and retry policies, not accelerated world UTC.
- Advance shared fixed-step movement and reconcile using server tick, ack/input sequence, movement-policy and collision revisions. Do not tie simulation semantics to redraw frequency.
- The world UTC estimator supports sky/environment presentation; it must not affect physics tick, network timeouts, cooldowns or durability scheduling.
- Keep current precision and units explicit (wire mm, predicted character state, world-space f64 when needed and presentation-only render origin).

## Review before changes

1. Trace current session/controller state, event producers, the desktop and terminal consumers, the world resolver and all cancellation paths.
2. State the before/after owner, allowed transitions, error codes, stale-result behavior and bounded resource policy.
3. For changed public API, update package README and callsites in the same PR; preserve current protocol/save/generator semantics unless an explicit separate issue authorizes change.
4. Check package graph and affected no-default-features client build when the toolchain is available. Never claim runtime handshake was exercised by a Cargo check.
