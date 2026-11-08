# Minimium vision

## Product direction

Minimium is a Rust engine for networked first-person games with a full 3D world model. Its long-range ambition is to support large multiplayer worlds while remaining small enough to study, operate, and profile. This is a direction for the project, not a statement that the current prototype is production-ready or meets a particular player-count or hardware target.

The client presents the world and gathers player input. A separate server process owns authoritative gameplay state, validates actions, and performs persistence. Local play uses that same process boundary and game protocol. Presentation may use stylized or 2.5D art while collision, movement, and world data remain three-dimensional.

## Accepted scope for the current implementation

The current implementation uses game protocol v18, world snapshot format v10,
and generator revision 5. It includes versioned world manifests, deterministic
generation, separate world instances, membership epochs, portal transfer, saved
player/return locations, durable entities and world-clock state. The client and
server are separate processes; the server is authoritative. Other protocol,
save, or generator revisions are rejected; no migration is provided. Game
message definitions remain independent of their transport adapter.

This scope excludes accounts, public-game authentication, TLS, inventory, NPCs, zones, a map editor, audio, and a browser client. These are not implied by the long-range product direction. Changes to protocol or save formats require an explicit decision and migration plan.

## Current prototype

The workspace contains a desktop client, headless server, admin CLI, load generator, and shared engine crates. Game traffic uses protocol v18 over TCP. The server runs a fixed 20 Hz simulation step, accepts a bounded number of active and pending connections, uses a 32-unit spatial grid for nearby queries, and produces bounded nearby-player snapshots. A local client can launch a separate server process or connect to an address. The default worlds are a seeded planet and a separate dungeon instance.

The local client profile ID is a durable identifier, not an account credential.
The sample server's developer authentication treats it as a claim; this is not
public account authentication. Map fingerprints and generator digests detect
accidental client/server mismatch; they do not authenticate a map or a player.
Save v10 records loaded manifests and fingerprints, player and entity state,
world-clock state, and initialized content. Older or unknown saves are rejected
without migration. Public account authentication, encryption, geometry edits
and inventory remain future work.

Execution lanes consist of the main server lane and optional persistent workers. One lane executes work inline. Storage snapshots and disk writes are handled by a dedicated worker outside the simulation loop. These are current implementation choices and measurement points, not evidence that more lanes improve performance or that save activity cannot affect observed runtime behavior.

## Future direction

The architecture may grow toward transport adapters beyond TCP, production authentication and encryption, cooked assets, authoring tools, additional gameplay systems, and multi-zone hosting. Each requires its own design and scope decision. The protocol crate should remain independent of socket and runtime details so a future transport can be evaluated without redefining gameplay messages by assumption.

The research target is 500 connected players on a Pentium-class 500 MHz server under a specified scenario. It is an unverified target, not a capacity claim. Hardware, operating system, compiler profile, lane count, workload, player distribution, and high-percentile latency must be recorded in a repeatable final verification phase before making performance conclusions.

## Evidence and verification

Implementation review and compilation can establish that code meets local structural and build requirements. They do not establish gameplay behavior, lane-equivalent outcomes, performance, or production readiness. The user explicitly started Epic 003's final verification phase: its workspace tests and wire-level planet → dungeon → planet restart/reconnect scenario passed. Performance measurements and the rendered-client transfer review remain open; no capacity claim is made.
