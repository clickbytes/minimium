---
name: minimium-new-system
description: Design and implement a new modular Minimium engine or game system with explicit owner, lifecycle, execution stage, inputs, bounded effects and application registration. Use for new mechanics/components/modules; not for simple refactors or shader-only changes.
---

# Add a new system without breaking engine boundaries

Read root/nested `AGENTS.md`, `architecture.toml`, the owner README and `crates/gameplay/README.md` first. This skill describes a repeatable implementation procedure; it is not an instruction to add a new crate or ECS.

## When to use

- Implementing a new game/system capability, registered server gameplay module, typed component, scheduled deterministic behavior or reusable resource owner.
- Introducing a new feature where domain ownership and lifetime must be designed from scratch.

## When NOT to use

- Purely moving/refactoring existing behavior: `minimium-safe-refactor`.
- Only editing presentation shaders or tuning atmospheric optics: `minimium-rendering`.
- Read-only architecture audits or static verification: `minimium-boundary-review` / `minimium-verify`.

## Workflow

1. Confirm current HEAD, policy, existing similar features and true owner. Define the user-visible objective, non-goals, expected inputs/outputs and affected server/client/game packages. Search for an existing system/renderer/worker **before creating one**.
2. Classify each part: **component** (schema + world data), **system** (behavior), **module** (static registration), **resource** (state/lifetime/budget), **adapter** (I/O/network/GPU) or **application composition** (game-specific choices). Do not require ECS/network entities for a purely cosmetic effect.
3. Declare an authoritative state owner, permitted mutators/read-only snapshots, identity/revision, units and appropriate time domain. Specify deterministic seeds, resource limits, cancellation and error policies.
4. For registered gameplay: inspect the real `minimium-gameplay` API—`ModuleRegistry`, `EngineModule`, `ModuleRegistrar`, `ModuleDescriptor`, `SystemDefinition`, `Stage`, `GameplaySystemContext` and `WorldEffectBuffer`. Use actual stages `Startup`, `FixedUpdate`, `Environment`, `PostSimulation` and `before/after` dependencies; validate duplicate/missing/cyclic IDs.
5. Design bounded operations through existing read-only contexts and effect buffers. The current gameplay buffer is capped at 256 operations and 1 MiB per stage; host validation commits effects on the server owner lane. Never pass unrestricted mutable `ServerRuntime` or worker/GPU handles to a registered system.
6. Connect application composition and data: `apps/server` supplies game modules and static configuration; `games/sample` owns sample-specific presets, portal/spawn/balance; client application adapts read-only snapshots to local presentation.
7. Explicitly map new behavior against built-in owner-tick operations. A registered hook does not automatically interleave with every current built-in tick operation. Do not invent a scheduler API; split a required scheduler extension into a separately scoped, approved change.
8. If a command is appropriate, use `minimium-command-surface` to define typed intent, permission, result and frontend exposure. If the system changes geometry or transfers, apply `minimium-world-consistency`.
9. Implement the smallest complete producer→registry→runner→consumer route. Remove dead stubs, check feature-gated packages and update README/architecture when the public contract changes. Apply applicable static checks with recorded exit status; defer tests/runtime acceptance under root AGENTS.

## Example

**Input A:** `Add a server-authoritative collectible system configured by sample-game.`

**Expected design (illustrative):** world-owned component schema and validated bytes; game module with `Startup`/requested interaction system and bounded effects; server-owner commit; explicit permission and replica visibility; sample balance and spawn composition; client renders a snapshot, never approves collection.

**Input B:** `Add another short-lived local sky sparkle.`

**Expected design:** client-owned monotonic event generator, bounded presentation frame and renderer pass, no server entity, packet, collision or save. Keep existing ambient meteor/bolid GPU composition rather than creating a second renderer.

## Completion and evidence

Provide owner diagram, component/system/module classification, concrete registration and callsites, stage/dependencies, typed input/output/errors, identity/clock/budgets, cleanup and compatibility guarantees. Run applicable static checks and report `NOT RUN` honestly for unavailable tools; final tests and GPU review remain deferred until authorized.
