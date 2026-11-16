# Client application instructions

Applies to `apps/client/`; supplement root `AGENTS.md` and the package README. This is the **composition/application** shell, not a new owner of authoritative game rules.

## Host and frontend responsibilities

- Desktop and terminal hosts configure the same `minimium-client-runtime` session/connection interfaces and application-provided world-content resolver. Windowing, input-device events, console editing, audio and frame scheduling live here; protocol validity, reconciliation and network lifecycles belong to the runtime.
- `minimium-client-runtime::ClientApplicationCore` owns session/world/prediction/reconciliation/action state and fixed-step dispatch. `ClientApplication` is its host facade plus settings, console/input and presentation transition state; it must not duplicate logical state or expose a mutable core/controller/game bypass. GPU resources belong to renderer owners.
- Normalize physical input, developer stdin, console and terminal into typed actions. A logical operation should reach one validated executor and typed result. Never execute a command by writing text into a widget or use rendering readiness as an implicit authorization rule.
- Desktop may require GPU asset readiness, terminal may not; both must implement the same protocol Prepare/Ready/Commit/Cancel contract using host-specific readiness adapters.
- Keep `--no-default-features` terminal operation independent of GPU, winit and astronomy FFI; prefer shared CPU value contracts. Preserve local-server ownership and admin permission boundaries: local developer mode must never grant remote privilege.

## Presentation and game content

- Rendering consumes prepared, context-bound client/world/environment views. Do not put persistent world edits, combat policies, time authority or storage into a shader, UI handler or client-only cache.
- Cosmetic ambient meteors and optional audio are client-local effects; no implicit world entity, protocol message or save entry. Tracked impacts and server bolids retain separate authoritative origin and can reuse visual composition.
- Content recipes and gameplay rules come from `games/sample` (or another game composition), never from a new engine hardcode. Host should adapt sample definitions to reusable domain and renderer ports.
- Do not duplicate the sample portal/interaction logic when adding a new frontend or CLI command.

## Change and evidence

1. Inspect the current producer, both desktop and terminal consumers, session events and lifecycle, input map, command registry and renderer integration.
2. Define typed input/query/effect, owner and permission; distinguish requested/queued state from confirmed server result.
3. Update complete callsites, no-default-features boundaries and documentation in one PR. Avoid a fake second application state owner.
4. Report source/feature checks separately from unperformed runtime/visual acceptance; follow the root `AGENTS.md` final-testing gate.
