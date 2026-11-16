# minimium-gameplay

## Purpose / Non-goals

Owns neutral descriptors and validated registration for modules, commands, systems and component schemas. It has no runtime, persistence, transport, process, filesystem, or graphics responsibilities.

## Public API

`ModuleRegistry`, `EngineModule`, `ModuleRegistrar`, and the command, system and module descriptors define static extension contracts. Game modules register commands with the neutral `ModuleCommandHandler` and return `ModuleCommandResult`; `ModuleCommandText` bounds UTF-8 text to 8 KiB with the existing ` [truncated]` marker, while `ModuleCommandErrorCode` validates stable codes. A host adapts the neutral handler into its own registry during transactional installation. Systems use `GameplaySystemContext`, a host-neutral trait, so game packages can implement systems without depending on a server runtime. They declare `before`/`after` constraints and run in stable topological order with system ID as a deterministic tie-break; missing, duplicate and cyclic dependencies return typed registration errors. The bounded `WorldEffectBuffer` carries entity/component changes and transfer requests; it accepts at most 256 operations and 1 MiB of component payload per stage. Each accepted `emit` returns a producer-scoped `EffectToken` containing stable module/system or module/command identity and a nonwrapping local sequence. `EffectBatchOutcome::receipts` pairs each token with its ordered typed result. Spawn results contain their `WorldInstanceId` and `EntityId` only after the entire entity subgroup passes staging, snapshot reservation and publication. The batch also retains its saturating aggregate count of buffer-rejected pushes. The system context exposes the current simulation tick, stage, optional startup world and New/Restored lifecycle, read-only world/component and initial-entity data, prior receipts and gameplay events, without process or I/O handles.

Startup runs once for each prepared world instance. Startup effects are validated and committed for that world before the runtime marks its initial content as initialized. A restored world receives `WorldLifecycle::Restored`; a world with no saved initialization marker receives `WorldLifecycle::New`.

The context offers a borrowed component lookup by world, entity and type for
interaction handlers, avoiding a full component scan per event.

System receipts arrive at the next invocation of the same registered system.
`GameplaySystemContext::take_effect_receipts` consumes only that producer's
results; leftovers expire when the invocation returns and increment the
runtime's cumulative `expired_effect_receipt_count`. Ordinary inboxes are
bounded to 256 receipts per producer and 4096 total. The runtime reserves
receipt capacity before accepting an effect, so full inboxes return typed
backpressure before commit; the host can inspect cumulative refusals through
`ServerRuntime::backpressured_effect_count`. Backpressure does not mark the
shared stage buffer overflowed or reject effects accepted from other producers.
Startup does not rerun to deliver feedback. Startup
receipts use a host-owned inbox capped at 4096 pending plus reserved receipts;
hosts drain one producer with
`ServerRuntime::take_startup_effect_receipts(module, system, world)`. Shutdown
returns any remaining receipts in `ShutdownOutcome::effect_receipts`. If
initialization fails after per-world startup commits, the constructor error
contains a downcastable `StartupEffectFailure` with all accumulated receipts and
the rejected-push count. Command
receipts remain ordered and return directly to the caller, without a duplicate
system inbox. Receipts are volatile execution feedback, not a durable journal;
they make no exactly-once promise across process restarts.

Systems can discover worlds through `GameplaySystemContext::worlds` using the
same `CommandViewRequest`, `CommandViewPage<CommandWorldView>` and
`CommandViewError` as module commands. Each call is bounded by the shared row
and byte caps; a returned cursor continues the same world-membership revision,
and membership changes produce `StaleCursor`. The reader is borrowed only for
the current system invocation, so systems retain cursor values rather than a
registry or runtime reference. This read does not change per-world Startup
handling or cause normal stages to dispatch once per discovered world.

Read-only module commands share `CommandViewRequest`, `CommandViewPage` and an
opaque `CommandViewCursor` for worlds, players, per-world entities, components
and the mixed entity/player listing. Requests accept 1..=256 rows and at most
1 MiB of dynamic data. Pages return a typed next cursor or `None`; invalid
limits, stale/mismatched cursors, unknown owners and a single row larger than
the byte cap are typed errors. Cursors are tied to the live reader, query,
scope and membership revisions. Membership stays consistent between pages;
mutable values such as player position are read live on each request.

## Dependencies

Depends on generic command metadata and world component descriptors. It does not depend on `minimium-server-runtime`, `minimium-storage`, transport, or GPU packages.

## Data flow

The host registers an `EngineModule` through `ModuleRegistry`; the registry
validates dependencies, command/system IDs and component descriptors before the
runtime installs handlers. Hosts can use `install_mapped` to adapt command
handlers while preserving transactional validation of names, dependencies,
systems and components. `examples/neutral_command_module.rs` demonstrates a
game module using only this crate's public API. At each owner-lane stage, systems receive a
`GameplaySystemContext` and write bounded `WorldEffectBuffer` operations. The
host validates and commits them with its world, persistence and replication
state. `examples/world_discovery.rs` demonstrates a neutral Environment system
with a finite four-page budget, typed stale-cursor handling, and a follow-up
terrain lookup using a discovered world ID. Its safe server-only marker
component uses an ID declared by the example's own registration.

## Ownership and execution

The server runtime owns the tick and supplies the current tick and per-world
Startup lifecycle. This crate owns only registration and effect contracts.

`ModuleRegistry::ordered_schedule` validates one graph containing registered
systems and the stable `BuiltinOperation` IDs. Its immutable `SchedulePlan`
supplies the runtime's actual operation sequence and ordered stage handlers.
Built-ins have fixed edges in the order below; stage systems are bounded by the
listed owner operations and their effect-publication barrier:

| Stage | After | Before publication |
| --- | --- | --- |
| Startup | Per-world restore (outside repeated tick) | `engine commands` |
| FixedUpdate | `engine world inputs` | `engine fixed effects` |
| Environment | `engine simulation` | `engine environment effects` |
| PostSimulation | `engine environment effects` | `engine post simulation effects` |

The full tick skeleton is commands → bolids → impacts → excavation → shutdown
gate → world inputs → fixed effects → simulation → environment effects → post
simulation effects → clear events. `BuiltinOperation::id()` supplies the exact
reserved dependency names (all prefixed with `engine `). The host runs a stage's
ordered handlers together and commits its single bounded batch at the named
effects barrier; no owner operation can interleave within that stage. Startup
handlers remain per-world and do not run again in each tick.

Module `before`/`after` constraints resolve both system and built-in IDs. Missing
targets return `MissingDependency`; reserved-ID collisions return
`DuplicateSystem`; repeated target/edge declarations return `InvalidDescriptor`;
reverse phase edges return `InvalidPhaseOrder`; same-phase cycles return
`DependencyCycle`. Stable system-ID ordering breaks unconstrained ties. An
invalid graph never produces a plan. Construction uses memory proportional to
trusted installed descriptors/edges; the host retains the plan and does not
rebuild it per tick. No registration, effect capacity or clock policy changes.

## Invariants and errors

Registration errors identify duplicate IDs, missing dependencies or cycles.
Effect overflow rejects the stage as a whole; concrete world, budget, placement
and transfer checks remain with the server host. The host commits entity effects
as one atomic subgroup and reports the same typed rejection for each member if
validation or storage reservation fails. Transfer effects report Started only
after the prepare message is queued, or a typed rejection distinguishing
validation, an existing pending transfer, identifier exhaustion, unavailable
client/environment, and backpressure. Starting a transfer does not report
transition readiness or world commit, and mixed batches do not promise
atomicity between entity storage and transport.

## Commands

Modules can register typed command descriptors and handlers. The host invokes
them with `ModuleCommandContext`, which exposes only bounded read views and a
bounded `emit(WorldEffect)` operation. World and entity pages cap at 256 rows;
player names and component payloads share a 1 MiB byte cap. Read pages report
truncation, while missing worlds and entities return distinct typed errors.
The context contains no runtime, registry, storage, or network handle. The host
owns command permissions and sends emitted effects through the same validated
executor used by gameplay systems. A handler error discards its whole effect
buffer. After execution, the command caller receives each effect's applied,
started, or rejected result; Started does not imply transfer readiness or
commit.

## Change guide

Keep module descriptions and registration generic. Host-specific handlers and world rules remain with their application or runtime owner.

## Verification

Use Cargo check for gameplay, sample and server-runtime, plus workspace static
checks. Test execution is deferred to the explicitly authorized final phase.
S09 acceptance scenarios include references before/after a real built-in,
reserved collisions, missing/repeated targets, cycles, phase reversals and the
shutdown/effect-publication order. Compilation does not establish these runtime
properties.

## Impact configuration and interaction port

Impact profiles are no longer module-registration hooks. The sample supplies a
profile constant and the application passes it explicitly through
`ServerComposition`; runtime validates it and owns scheduling.
`GameplaySystemContext::terrain` borrows the compatible crater snapshot;
`terrain_read` borrows the current planetary signed-field query, including
committed crater and volume edits when available. Missing worlds or unavailable
terrain remain unavailable rather than being treated as empty space.
`EntityInteractionRequested` is the generic interaction event; the sample portal
consumer accepts it and the earlier `PortalUseRequested` compatibility variant.
Inventory, mining rewards and damage policy are not implied by the impact profile.
