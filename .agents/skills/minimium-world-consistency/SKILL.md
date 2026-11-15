---
name: minimium-world-consistency
description: Trace and change world instance, sector, terrain, collision, spatial query or portal/transfer behavior in Minimium while keeping identities, revisions, active/pending content and all consumers coherent. Use for world-state consistency work, not shader-only or CLI-only changes.
---

# World, spatial and transfer consistency

Read root and applicable nested AGENTS, `architecture.toml`, `crates/world/README.md`, and relevant `simulation`, `sectors`, `terrain`, `fluids`, `procgen` and client/server runtime READMEs. Treat the actual code as evidence; do not assume an older audit finding still applies to current HEAD.

## When to use

- Changing `WorldDefinition`, `WorldInstance`, `WorldRegistry`, `WorldContext`, sector streaming, terrain edit, collision, spatial queries or object replication.
- Debugging inconsistent terrain/collision/combat/render state after an edit, teleport, portal transition or async generation result.
- Refactoring a world-related owner or publishing versioned geometry, including water and crater integrations.

## When NOT to use

- Shader-only visual tuning without world data changes (`minimium-rendering`).
- Generic command/help changes (`minimium-command-surface`).
- An unrelated application/infra refactor (`minimium-safe-refactor`).

## Workflow

1. Record current HEAD and read real authoritative world owner, instance and manifest. Separate **definition** (identity/rules), **instance** (state/data) and **registry** (lifecycle/membership). Do not infer type from an instance ID or sample seed.
2. Trace input/mutation (authorized owner lane) → revision/content identity → affected sectors/tickets → immutable worker snapshot → staging → validated **atomic publication** → each collision, combat, fluid, replication, renderer and persistence consumer.
3. For sector changes, prove that version keys change whenever any dependency changes. Inspect content and terrain revision hashing for cancellation/collisions, cache invalidation and interleaved worker results; do not use a cheap XOR token as proof of uniqueness.
4. If multiple spatial indices exist, they may have different query radii and update costs. Preserve them when justified, but require the same latest authoritative geometry revision at the boundary. Do not replace combat's longer-range index with a shorter-range movement cache.
5. For portal/teleport: keep `ActiveWorldContent` and `PendingWorldContent` conceptually separate (names are design terms, not promises of current Rust types). Follow `Prepare → Ready → Commit/Cancel/Fail`; confirm context, ticket, manifest/fingerprint, readiness and cancellation of late work. Do not install pending geography in the active world.
6. For physics: use the world-specific gravity/frame, not an implicit global +Y. Track units and precision (mm serialized/domain values; f64 planetary/world measurements; f32 relative GPU/visual coordinates), and use the same character/capsule basis for movement, weapon rays, tools and camera.
7. Check the time domain: real monotonic deadlines/cooldowns versus fixed simulation ticks versus world UTC/time-scale. A fast sky clock must not accelerate gameplay admission or autosave.
8. Preserve protocol version, save format, generator digest/seed, world fingerprints and asset identity unless a separately authorized issue approves migration. Inspect current actual versions rather than copying historical numbers.
9. Update all producers/consumers and owner README in the same change. Use `minimium-safe-refactor` or `minimium-new-system` as applicable, then `minimium-verify` for static evidence. The user's final testing gate remains in force.

## Example

**Input:** `Investigate why terrain edited in one sector is visible to movement but not weapon hitscan.`

**Illustrative diagnosis plan, not a claim about current HEAD:** compare the installed geometry revision in the movement and combat indices, trace streamed-sector refresh and authoritative terrain publication. If the combat index is stale, publish its own correctly ranged index from the updated source, or provide one versioned spatial-query port. Do not blindly alias a short-radius index. Check remote/client replicas and shadows for the same version.

## Completion and evidence

Provide exact world/instance/context identities, before/after version graph, all affected query consumers, transfer/cancel behavior, coordinate and clock assumptions, error/rollback outcomes, preserved wire/save/seed constraints and actual static-check statuses. Note that static analysis alone cannot prove runtime collision correctness.
