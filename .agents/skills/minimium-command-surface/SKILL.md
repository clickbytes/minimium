---
name: minimium-command-surface
description: Add or refactor typed CLI, console, input actions, settings/properties or server administration commands in Minimium with shared owner-side execution, permission checks and predictable results. Use for command surfaces, not for shader-only or unrelated simulation algorithms.
---

# Typed operations across frontends

Read root/nested AGENTS, `crates/commands/README.md`, ADR 0007, `docs/commands.md`, the relevant `apps/client` and `apps/ctl` README and the server-runtime gameplay module contract. The currently registered names/aliases and schemas are authoritative, not historical examples.

## When to use

- Adding or changing a console/CLI command, `get/set/reset/list` property, startup/config value, input action or `minimiumctl` operation.
- Fixing a mismatch between desktop, terminal, developer stdin and owner handlers.
- Moving duplicated command parsing or permission decisions to a single typed executor.

## When NOT to use

- Renderer/WGSL-only work with no command surface change.
- A pure deterministic simulation algorithm with no public action/property; use `minimium-new-system` if implementing a feature.
- General architecture auditing without code changes.

## Workflow

1. Capture HEAD and current CLI/console/ctl command registry. Classify the intent before design: **gameplay action** (one event), **configuration property** (requested/effective), **administrative operation** (authorization + durable result), or **read-only query**. Never model every imperative action as an arbitrary writable string.
2. Declare the owner of value validation, mutation and source of truth: client-local presentation, player-requested server policy, authoritative world property or server admin. Preserve existing permission and local/remote boundaries.
3. Define a stable command/property descriptor: canonical name, aliases, typed arguments/value codec, units/ranges, help/completion metadata, scope, lifecycle (`live/restart_required/creation_only/read_only` where appropriate), persistence/default/reset policy, owner and error codes.
4. Search for collisions across built-in and module names/aliases. Place parsing and completion in the shared registry; keep host-specific formatting/localization in adapters, not in the domain rule or wire codec.
5. Implement one semantic pipeline: input/device or CLI text → typed intent → permission+owner-side validation → one executor → typed `Result`/event → host formatter. Do not inject command text into a GUI editor to execute it; do not let GUI-only guards be the only validation.
6. For networked action, distinguish locally **queued/requested** from server-confirmed **effective/committed** state. Never report success merely after enqueue. Handle asynchronous denial, stale world context, timeout, disconnect and partial success explicitly.
7. Resolve settings through their canonical descriptor/typed value representation: parse/serialize round-trip, layered requested/effective/source, correct reset to startup/config layer, and meaningful unavailability. Do not derive persisted keys/values from `Debug` output.
8. Review every relevant surface: desktop F1, terminal/no-default-features, developer stdin, server terminal, loopback admin/ctl and configuration preview. Document a host intentionally lacking an operation rather than silently creating a different behavior.
9. Preserve wire/save/protocol format, authorization and seeds unless a separately approved issue changes them. Update README/help/ADR only when ownership or public contracts change; run applicable static checks and report test/runtime gaps.

## Example

**Input A:** `Fix client.debug.atmosphere reset for the optical_depth value.`

**Expected shape (illustrative):** one enum parse/format codec accepts and emits `optical_depth`; startup and session overrides store the typed/canonical value; reset obtains the saved startup layer without reconstructing it from `Debug`; an invalid input yields a typed error, not panic.

**Input B:** `Expose world.time.scale to minimiumctl.`

**Expected shape:** authorize via the owned server-admin channel, validate time scale in its domain owner, return a typed result; do not grant remote users admin rights through the player console.

## Completion and evidence

Record every command alias and consumer, owner/permission/lifecycle, requested/effective semantics, errors and feedback, added/removed execution paths, feature-gated CLI variants, help/completion updates and exact static checks. Unit/E2E and runtime permission checks remain deferred until explicitly authorized.
