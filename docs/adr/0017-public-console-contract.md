# ADR 0017. Public console operations

Accepted for E051 on 2026-11-19. Baseline `fb626d32944b373dd995a5c703ee98eef0db9b35` uses protocol 22 and save 12. This decision governs the implementation sequence in [E051](../plan/epic-051.md).

## Decision

Use a stable path and typed operation identity across desktop F1, developer stdin, terminal, admin control and scripts. Name a field or method from its state owner, scope, read source and commit boundary. Each intermediate segment identifies a recognizable subsystem or runtime context. A private Rust field move preserves the public name. A change of authority requires a documented migration with aliases, owner checks and explicit provenance. For example, `player.movement.speed` names a policy on the current session actor even when a client holds the latest confirmed value.

Compose immutable descriptors from the existing command and property registries at startup. Domain modules supply schema, access, help, units, persistence and bindings. The host supplies available capabilities. Values, validation and authority remain in the existing owners. A collision rejects the whole registration. Help, completion and explanation use the same metadata. A namespace only groups children.

Keep [ADR 0007](0007-command-registry.md) for registration and [ADR 0016](0016-committed-simulation-time.md) for the shared clock and its committed boundary. An admin snapshot and a player's replicated sample can expose the same operation with distinct provenance. Simulation authority stays with the server.

## Alternatives and costs

Separate host parsers already disagree about developer commands, permissions, scalar decoding and when a result counts as success. A declarative catalog costs a schema and an adapter at each owner. It also allows hosts to discover a new operation from one registration. Unreal's registered console variables illustrate the benefits of names, types, help and source tracking. Minimium keeps the live values in its domain owners. [Unreal console variables](https://dev.epicgames.com/documentation/en-us/unreal-engine/console-variables-cplusplus-in-unreal-engine)

A macro language would require sequencing, error handling and cancellation rules. Conditional restore and waiting for authoritative results already require these facilities. Use the declarative syntax for individual operations and the default-off Rhai client adapter for workflows. Its costs include compilation, a worker, limited host calls and a bounded AST cache. The engine and runtimes stay independent of the interpreter. Rust registers typed functions and the worker caps evaluated operations as well as host-side work. [Host functions](https://rhai.rs/book/rust/functions.html) · [Operation limits](https://rhai.rs/book/safety/max-operations.html)

## Compatibility and evolution

Preserve registered spellings and exact legacy rest text. Add documented missing aliases as explicit repairs. Normalize a quoted scalar once through its declared codec. Capture configuration provenance at startup. Reset removes a session override and exposes the captured value. Registering an operation never grants access to it.

Version schema identifiers and admin envelopes independently of the game protocol and save format. Dispatch a supported envelope once with its correlation, session and expected context. Reject unsupported versions before execution. Keep legacy text clients working. After ambiguous delivery a client reports an unknown outcome and retains any known commits. Player receipts changing the wire require an explicit protocol revision and compatible endpoint updates in their owning issue.

Verify extensibility by registering the achieved clock rate and its diagnostics through the domain module. The implementation diff and a compiled consumer must show that both operations reach the existing parser, help, completion and terminal without per-name changes. The later script bridge must consume their schemas. [Contract](../console-api/contract.md) · [Acceptance](../console-api/verification.md)
