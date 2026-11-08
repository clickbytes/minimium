# minimium-gameplay

## Purpose / Non-goals

Owns neutral descriptors and validated registration for modules, commands, systems and component schemas. It has no runtime, persistence, transport, process, filesystem, or graphics responsibilities.

## Public API

`ModuleRegistry`, `EngineModule`, `ModuleRegistrar`, and the command, system and module descriptors define static extension contracts. A host supplies handler types and installs modules in dependency order. Systems use `GameplaySystemContext`, a host-neutral trait, so game packages can implement systems without depending on a server runtime. They declare `before`/`after` constraints and run in stable topological order with system ID as a deterministic tie-break; missing, duplicate and cyclic dependencies return typed registration errors. The bounded `WorldEffectBuffer` carries entity/component changes and transfer requests; it accepts at most 256 operations and 1 MiB of component payload per stage. The context exposes the current simulation tick, stage, optional startup world and New/Restored lifecycle, read-only world/component and initial-entity data, and gameplay events, without process or I/O handles.

Startup runs once for each prepared world instance. Startup effects are validated and committed for that world before the runtime marks its initial content as initialized. A restored world receives `WorldLifecycle::Restored`; a world with no saved initialization marker receives `WorldLifecycle::New`.

The context offers a borrowed component lookup by world, entity and type for
interaction handlers, avoiding a full component scan per event.

## Dependencies

Depends on generic command metadata and world component descriptors. It does not depend on `minimium-server-runtime`, `minimium-storage`, transport, or GPU packages.

## Data flow

The host registers an `EngineModule` through `ModuleRegistry`; the registry
validates dependencies, command/system IDs and component descriptors before the
runtime installs handlers. At each owner-lane stage, systems receive a
`GameplaySystemContext` and write bounded `WorldEffectBuffer` operations. The
host validates and commits them with its world, persistence and replication
state.

## Ownership and execution

The server runtime owns the tick and supplies the current tick and per-world
Startup lifecycle. This crate owns only registration and effect contracts.

## Invariants and errors

Registration errors identify duplicate IDs, missing dependencies or cycles.
Effect overflow rejects the stage as a whole; concrete world, budget, placement
and transfer checks remain with the server host.

## Commands

Modules can register typed command descriptors and handlers; the host owns
execution and command permissions.

## Change guide

Keep module descriptions and registration generic. Host-specific handlers and world rules remain with their application or runtime owner.

## Verification

Compilation and static validation are deferred to the workspace final verification phase.
