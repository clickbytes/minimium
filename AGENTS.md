# Project workflow policy

## Tests

- Unit, integration, and end-to-end tests are deferred until the project's final verification phase.
- During normal implementation, do not add test suites or run tests. Treat testing as a project-wide final phase, not as a requirement for each feature or milestone.
- Begin that phase only when the user explicitly says the project is ready for final testing.
- Compilation, formatting, and static checks are separate from tests and may be used during implementation when useful.

## File size

- Hand-maintained code and shaders are limited to 500 physical lines per file. Do not evade the limit by combining lines.
- Every exception must document the specific reason and its scope. Generated files and data assets follow separately documented rules and must not be used to hide hand-maintained code.

## Domain ownership

- Read `architecture.toml` and the owning package's `README.md` before changing domain code.
- `docs/architecture.md` distinguishes current Cargo ownership from planned package boundaries; do not create empty placeholder crates.
- Keep domain code free of process, filesystem, transport, GPU and UI effects unless its declared package responsibility is an adapter or application.
- When moving behavior, keep the old consumer connected in the same change and update the ownership declaration and package README.
- Preserve existing world, protocol, save and generation behavior during structural changes.

## Mission and source of truth

Minimium is a reusable Rust engine with an authoritative server, separate client runtime, typed operations and presentation adapters. Prefer predictable ownership, small interfaces and complete migrations to adding another special-case path.

- `architecture.toml` declares **actual** Cargo roles and forbidden dependencies.
- The owning package's `README.md` describes its current public API, state owner, data flow, invariants, error handling and verification.
- `docs/architecture.md` describes integration and planned direction; `docs/adr/` records accepted decisions. For agent governance consult [ADR 0014](docs/adr/0014-agent-engineering-governance.md).
- A matching `.agents/skills/*/SKILL.md` (when present) describes **how** to carry out a recurring task, not facts about an API. See `docs/agent-workflow.md` when it exists.
- Work from the current branch and commit. Read only relevant nested `AGENTS.md`, the owner README, callsites and ADR. When a described symbol is absent, inspect the code rather than inventing it.

## Engine design rules for changed code

1. Each mutable domain state has one authoritative owner; access it through controlled operations, not public field mutation from arbitrary modules.
2. Server simulation, admission and persistence are authoritative. Client prediction, snapshots and GPU views never approve gameplay/world mutations.
3. Keep `WorldDefinition`, `WorldInstance`, `WorldRegistry`, active/pending world content, revision and `WorldContext` distinct. Publish coherent versions for consumers of geometry and sectors.
4. Domain models must have no filesystem, network, process, UI or GPU side effects. Adapters and applications may perform their **declared** effects; `minimium-renderer` owns presentation/GPU, not sessions or combat rules.
5. Engine/runtime behavior must not depend on `minimium-game-sample`. Scenario, rules, spawn and content are supplied by application composition.
6. Input, CLI, in-game console and admin transports may differ in permissions or formatting, but a logical operation should use typed intent, owner-side validation and a typed result. Queue acceptance is not authoritative success.
7. Systems/modules expose their inputs, outputs, execution phase, dependencies, lifecycle, errors and bounded resources. Map current built-in tick behavior before altering the schedule.
8. Use explicit units and clocks: monotonic real time for lifecycle, fixed simulation steps for physics, UTC/world time for astronomy and environment, and render-origin-relative GPU coordinates.
9. Favor composition, narrow API surfaces, validated constructors and typed errors. Avoid hidden mutable singletons, double owners, unnecessary ECS/crates/trait layers or duplicate renderers.
10. These are rules for new changes and refactors, not assertions that all legacy code already meets them. If a violation is found, document the existing debt rather than silently expanding task scope.

## Implementation workflow

1. Establish the current HEAD, affected files, actual owner and every producer/consumer before editing. Do not overwrite unrelated or uncommitted work.
2. State preserved behavior, failure/cancellation semantics, protocol/save/generator/asset compatibility and the smallest complete change.
3. Connect old consumers to the new owner in the same change and remove obsolete duplicate paths. Do not leave unused placeholder APIs.
4. Change `architecture.toml` and the package README in the **same change** when public ownership/contracts genuinely change. Record major decisions as ADRs.
5. Review the diff for accidental behavior changes, feature-gate regressions and violations of the 500-line limit.
6. Run only applicable, available **static** checks. Typical commands: `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `cargo check -p minimium-client --no-default-features --locked` (client-only), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py`, `python scripts/check_static_refactor.py`. They do not replace the deferred test phase.
7. Report the exact changed files and owner; invariants preserved/changed; commands with exit status `PASS` / `FAIL` / `NOT RUN` / `NOT APPLICABLE`; unresolved issues and runtime/visual verification gaps. Never claim an unexecuted check passed.

## Issues and integration

For an ordered Epic, do not start a dependent issue before its predecessor is verified **CLOSED**. Use a small complete PR/change, review the actual diff, merge only after acceptance criteria are met and verify the result on `main` before closing the issue. Do not claim an issue was implemented just because it was described in a plan.
