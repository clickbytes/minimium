# ADR 0014: Agent engineering governance

Status: accepted for E048. Date: 2026-11-15.

## Context

Minimium is a Rust game engine with server-authoritative simulation, separate client runtime, a sample-game composition layer and renderer adapters. Its existing governance is already expressed by `AGENTS.md`, `architecture.toml`, package READMEs and ADRs 0004/0008/0012. ADR 0013 belongs to **ambient meteors** and is not replaced by this decision.

Repository state examined for this decision: GitHub `main@93f254d04e0860516301af757a3ad1a1aeb0ecad`. Its source declares protocol v21, save v11, generator revision 6, `minimium-gameplay`, `minimium-terrain` and `minimium-fluids` alongside the existing runtime, presentation and adapter packages. A GitHub repository connection cannot inspect a developer's uncommitted working tree. Every later implementation must re-read current HEAD and relevant sources; the baseline SHA is not a checkout instruction.

The objective is predictable, reviewable changes by coding agents. Instructions alone cannot prove semantic correctness, so they must complement—not replace—type visibility, narrow APIs and the established static architecture checks.

## Decision: one authoritative document for each kind of fact

| Fact or instruction | Canonical owner | Consumers |
| --- | --- | --- |
| Project-wide rules, testing phase, 500-line code/shader limit and delivery evidence | Root `AGENTS.md` | Every agent/task |
| Additional local restrictions for specific paths | Nearest nested `AGENTS.md` | Agent editing that subtree |
| Actual Cargo role and forbidden dependency edges | `architecture.toml` | Library author, architect, architecture checker |
| Current public contracts, ownership, lifecycle, API and invariants | Owning package `README.md` and actual code | Producer, consumer, reviewer |
| Why a cross-cutting decision was chosen | `docs/adr/` | Architecture change/review |
| **How** to execute a recurring task (review, refactor, new system, render, CLI, verify) | `.agents/skills/<skill>/SKILL.md` | Agent when task matches skill description |
| Current scope, sequence, acceptance and evidence for a change | GitHub Epic/Issue/PR | Implementer/reviewer |
| Routing between these sources and tooling capability caveats | `docs/agent-workflow.md` | All contributors |

The repository root policy always applies. Nested instructions narrow and supplement it, never relax it. Actual code and `architecture.toml` determine existing implementation facts; README describes those facts and is corrected when stale. ADR defines accepted intent. A skill is a workflow and must not invent a missing library, tool or API. When instructions disagree, record the conflict and reconcile the **owning** source before proceeding with a change that depends on it.

## Loading policy and skill discovery

1. Start with the current branch, commit, diff and task. Inspect only the relevant files.
2. Read root `AGENTS.md`, then the nested instruction(s) covering edited paths.
3. Read `architecture.toml` and the relevant owner package README; inspect actual producers, consumers and callsites.
4. Load a skill only when its concise `name`/`description` matches the operation. Confirm both "When to use" and "When not to use" inside that skill. Do not load every skill.
5. Consult an ADR only for a decision implicated by the change. If a runner cannot discover repository-local skills automatically, explicitly open the corresponding `SKILL.md`.
6. For a cross-domain change identify the responsible owner and all affected readers/writers before editing. Prefer complete narrow migrations to parallel implementations.

## Architectural invariants

- Each mutable domain state has one authoritative owner and a controlled mutation path. Server simulation, persistence and admission remain authoritative; client prediction and rendering do not authorize world edits.
- Game configuration and sample content belong to application composition. Runtime and engine crates must not depend on `minimium-game-sample`.
- Foundational domain contracts have no process, filesystem, transport, GPU or UI side effects. Legitimate platform, storage, transport and GPU adapters retain their declared responsibilities.
- `WorldDefinition`, `WorldInstance`, `WorldRegistry` and context-bound active/pending world content are distinct. Cache and published geometry versions must match their dependencies; movement, combat and presentation must observe consistent data.
- Real monotonic time for network/lifecycle and simulation steps remains distinct from world UTC/`time_scale` used by sky/environment. Units and render-origin transformations must be explicit.
- CLI, console, input and administrative transports may have different permissions, but equivalent logical operations go through typed commands, owner-side validation and typed results.
- A registry entry is not proof that a system participates in the full tick order; existing built-ins must be mapped before changing scheduling.
- Do not introduce an ECS, empty crate, second renderer, global singleton or additional framework without a demonstrated ownership need.

## Execution and evidence

One dependent E048 issue is implemented and reviewed at a time, with its previous issue **CLOSED**. For each issue: read current HEAD and policy → identify owner/callers and expected unchanged behavior → make the smallest complete change → inspect diff → run applicable static checks → document exit status and limitations → merge and verify on `main` → close the issue. If a prior stage proves incomplete, reopen/fix it before proceeding.

Allowed during regular work: `cargo fmt --all -- --check`, `cargo check --workspace --locked`, applicable feature-specific Cargo checks, architecture/file-length/static-refactor scripts, shader static validation where available. These are **static checks, not unit tests**. Per the root `AGENTS.md`, do not add or run unit/integration/E2E test suites or perform game/GPU visual/benchmark acceptance until the user explicitly authorizes the final testing phase. When a runner cannot execute a check, report `NOT RUN` and the exact reason—not `PASS`.

Evidence in every closing issue: current SHA and PR/merge SHA, changed files, owner and invariants, concrete commands with `PASS`/`FAIL`/`NOT RUN`/`NOT APPLICABLE`, manual diff findings, unresolved risks, and confirmation that unrelated production behavior was not altered. Source review and regex guards can detect selected violations but cannot prove runtime compatibility or correctness.

## Ownership/routing examples

| Task | Read first | Workflow | Expected result |
| --- | --- | --- | --- |
| Move client session behavior out of `Renderer` | `apps/client`, `client-runtime` READMEs and caller code | `minimium-safe-refactor` | One owner/executor, migrated callers; no UI-dependent session semantics |
| Add an authoritative game system | `gameplay`, `server-runtime`, `simulation` READMEs | `minimium-new-system` | Explicit inputs, effects, phase, registration and app composition |
| Fix transfer/sector geometry mismatch | `world`, `sectors`, `terrain`, `simulation`, client/server runtime | `minimium-world-consistency` | Coherent versioned state, active/pending isolation, consumers updated |
| Change cloud/shadow WGSL composition | `renderer`, `environment` contracts, current pass graph | `minimium-rendering` | Bounded resource ownership, valid layouts, HDR/depth consistency |
| Expose a configuration or admin action | `commands`, `apps/client`, `apps/ctl`, `server-runtime` READMEs | `minimium-command-surface` | Typed value/permission/result, shared execution and host-specific presentation |

The planned skills and nested `AGENTS.md` are delivered in E048-01..15; a document's presence must be checked rather than assumed at this baseline. E048-16 verifies integration and merge. No engine/runtime implementation, world seed, protocol, save, generator or asset identity change is authorized by this governance ADR.

## Consequences and maintenance

Root policy changes require cross-repository review; ownership changes update `architecture.toml` and the owning README in the same PR; decisions changing architectural direction update or supersede an ADR; repeated workflow improvements update one skill; the routing catalog changes when a skill is added/removed. The lightweight static skill checker validates file shape and selected cross-references only. Avoid duplicating large policy blocks across local instructions and skills: the shortest current, correct instruction set is preferable to a large stale one.
