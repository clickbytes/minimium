# Server runtime instructions

Applies to `crates/server-runtime/` in addition to root/crates `AGENTS.md`. Read this package's `README.md`, `architecture.toml`, the relevant `minimium-gameplay` contract and current callsites before changing server behavior.

## Authority and state ownership

- The server runtime is authoritative for world lifecycle, admissions, simulation, registered effects, transfers, persistence coordination and shutdown. Only the designated owner lane publishes mutable active-world state.
- Network admission validates principal permissions, request context, bounds and intent; it must not become a second, separately mutable source of gameplay rules.
- Worker threads prepare immutable, bounded results. They do not mutate live world registries. Publish only after validating context, revisions, aggregate budgets and committed write boundaries.
- `minimium-storage` owns save encoding and persistence workers; the runtime constructs immutable snapshots and propagates real completion/errors. Never interpret worker acceptance as durable success.
- `minimium-game-sample` is not a runtime dependency. Application composition supplies worlds, combat values, game-specific initial content and module list; do not hardcode demo spawn/portal/rules here.

## Modules and deterministic execution

- Use the actual `minimium-gameplay` descriptors, lifecycle stages, system effects, component schemas and dependency ordering. Verify registered systems and built-in operations against the **real** tick sequence before inserting work.
- Document each new system's owner, inputs, read views, phase, dependencies, bounded effect capacity and failure/cancellation semantics.
- Cross-system mutation goes through validated effects, staged batches or a narrow owner operation. Reject an invalid batch before partial publication; if an API cannot guarantee atomicity, return a typed, documented partial outcome.
- Do not expose unrestricted `&mut ServerRuntime`, `&mut WorldRegistry` or raw storage/network internals to plugins. Avoid hidden singleton registries and duplicate scheduling paths.

## World sessions, sectors and shutdown

- Keep `WorldDefinition`, `WorldInstance`, `WorldRegistry`, source/destination context and transfer ticket distinct. A transfer moves through staged preparation, readiness, explicit commit/cancel/fail and cleanup.
- Sector request/ticket/pin/release, generation workers, collision publication and snapshot revisions must remain coherent; do not reuse stale world/sector data after transfer.
- Preserve the real monotonic clock for server admission, cooldowns, autosave/shutdown deadlines and worker budgets. World UTC/`time_scale` controls the modeled environment, not persistence or network timing.
- Maintain bounded shutdown with explicit admission closure, draining, worker completion, snapshot capture and final result; a timeout is not proof that OS I/O stopped.

## Change review

1. Trace admission → authoritative owner → phase/effect → published result → replication/persistence and identify every consumer.
2. Record current world/context, permission, budget, commit/rollback and error behavior before modifications.
3. Integrate and remove alternate paths in the same PR; preserve existing protocol/save/generator/seed unless an authorized issue explicitly changes them.
4. Update the package README and architecture declaration only when the actual public contract/owner changes. Use applicable Cargo and static checks; report tests/runtime NOT RUN under root policy.
