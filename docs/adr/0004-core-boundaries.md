# ADR 0004: Domain and runtime boundaries

Status: accepted for the current prototype.

Core owns domain IDs, world state, collision and simulation; it has no filesystem, network, process, GPU or telemetry dependency. Protocol owns v8 wire kinds and errors and depends on core IDs. Assets loads map and OBJ data into the domain; platform owns filesystem, identity storage and local administration. Telemetry is independent. Applications compose these pieces; only the client owns the GPU and window stack. The server owns its executor, transport, persistence and runtime lifecycle.

`PlayerId` is a domain identifier and `PlayerProfileId` is the durable profile identifier. Explicit conversions at serialization boundaries preserve existing representations. Protocol enums own wire discriminants and retain unknown server error codes. These choices keep domain identity independent of transport and persistence without changing v8, snapshot v3 or v2 migration.

Storage remains a server module while its ownership and worker are server-specific. A generalized transport crate is deferred until multiple real transports justify a stable abstraction; the current codec already separates message bytes from TCP buffering. Existing platform FFI implements secure randomness and atomic replacement; telemetry FFI provides process sampling and optional allocation accounting. No platform FFI is needed in core. Static inspection and the Cargo graph checker do not establish runtime safety or compatibility; the scenarios in the epic matrix require the separately authorized final phase.
