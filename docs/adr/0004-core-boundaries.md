# ADR 0004: Domain and runtime boundaries

Status: superseded by [ADR 0008](0008-domain-crate-boundaries.md).

This records the earlier prototype boundary: core owned IDs, world state, collision and simulation. The accepted package boundaries have since been refined; see ADR 0008 for current ownership. The original constraints against filesystem, network, process, GPU and telemetry dependencies in foundational contracts remain in force.

`PlayerId` is a domain identifier and `PlayerProfileId` is the durable profile identifier. Explicit conversions at serialization boundaries preserve existing representations. Protocol enums own wire discriminants and retain unknown server error codes. These choices keep domain identity independent of transport and persistence without changing v8, snapshot v3 or v2 migration.

Storage remains a server module while its ownership and worker are server-specific. A generalized transport crate is deferred until multiple real transports justify a stable abstraction; the current codec already separates message bytes from TCP buffering. Existing platform FFI implements secure randomness and atomic replacement; telemetry FFI provides process sampling and optional allocation accounting. No platform FFI is needed in core. Static inspection and the Cargo graph checker do not establish runtime safety or compatibility; the scenarios in the epic matrix require the separately authorized final phase.
