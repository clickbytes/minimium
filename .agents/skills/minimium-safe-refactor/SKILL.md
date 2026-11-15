---
name: minimium-safe-refactor
description: Perform behavior-preserving Rust architectural refactors in Minimium, including moving ownership, narrowing interfaces and removing duplicated paths. Use when implementation must keep old consumers working; not for feature development or intentional protocol changes.
---

# Safe ownership refactor

Use root/nested AGENTS, the actual source, `architecture.toml` and package README. Consult `minimium-boundary-review` when the desired owner is not established. A file split is not an ownership refactor unless responsibility actually moves.

## When to use

- Refactoring existing Rust code for SRP, module boundaries, public field visibility, duplicate execution paths or excessive application/runtime coupling.
- Moving state or behavior to an existing rightful owner without intentionally changing player-observable behavior.
- Completing an already approved architectural migration with concrete affected producers and consumers.

## When NOT to use

- New mechanics, changed rules or planned protocol/save/generator versions: use an explicit feature/migration issue and `minimium-new-system` where appropriate.
- A read-only code audit: use `minimium-boundary-review`.
- Shader-only changes where the logic/ownership stays the same: use `minimium-rendering`.

## Workflow

1. Record current branch, HEAD, diff/working-tree visibility and the issue's intended result. Re-read active caller code; historical line numbers are hints.
2. Identify the authoritative state owner and every producer, reader, writer and side-effect adapter (including terminal/desktop, server/client, optional features, persistence and diagnostics).
3. Write a **before** contract: inputs, outputs, mutation rights, error codes, cancellation/failure behavior, order/phase, units, clocks, revisions, resource bounds and whether the operation is atomic.
4. Define the **after** contract: a narrow typed constructor/port/command/result at the correct owner. Prefer a private state and explicit method over exporting `&mut` internals. Do not add a crate, ECS, trait or singleton without a demonstrated use.
5. Implement the smallest complete vertical migration: make the owner available, switch every real callsite, preserve capabilities, remove old routes and avoid dual state. An unused interface, placeholder module or parallel dispatch path is not done.
6. Keep the observable behavior and stable identities constant: protocol fields/discriminants, save order/format, world seeds/digests, asset hashes, error/permission semantics, tick/clock behavior and GPU outputs. If a change cannot preserve one, stop and split an explicitly authorized behavior change.
7. Review cancellation, stale worker results, partial publication and retry. If atomicity cannot be guaranteed, the API must express a partial outcome rather than returning an unexplained success.
8. Update the owning README and `architecture.toml` only when the actual public ownership or dependency graph changed. Remove obsolete docs/exports; inspect `git diff` for accidental extra changes.
9. Apply `minimium-verify` when available: applicable static checks, 500-line limit, feature-gated builds, actual command exit codes and explicit unverified runtime/test items. The root testing policy remains in force.

## Example

**Input:** `Move client session transition control out of Renderer; do not alter protocol or gameplay behavior.`

**Required plan/output (illustrative, not proof of current code):**

| Boundary | Before | After |
| --- | --- | --- |
| Authoritative client transition | Renderer method and session callback both mutate ticket | Application/controller validates ticket and active context |
| Rendering | May access transient session state directly | Consumes immutable state and reports GPU readiness |
| Terminal | Separate ad-hoc ready handling | Uses the same lifecycle with its own readiness adapter |

List every modified callsite, deletion of old path, relevant unchanged invariants and exact check statuses. Do **not** state parity was tested when no runtime test was authorized.

## Completion and evidence

The change is complete only when all callers use one owner; unused implementations are gone; no new forbidden dependencies exist; current README/architecture agree with code; the PR diff is reviewed; static checks have actual `PASS/FAIL/NOT RUN/NOT APPLICABLE` results; and any behavior/compatibility risks are explicit. Do not close a dependent Issue before its predecessor is verified CLOSED.
