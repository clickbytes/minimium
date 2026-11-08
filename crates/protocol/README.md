# minimium-protocol

## Purpose / Non-goals

Owns versioned network message models, byte codecs, validation and frame limits. TCP buffering and gameplay decisions belong outside this crate.

## Public API

The only supported protocol version is 19. It uses a bounded provider credential message and requires every gameplay action to carry an explicit `WorldContext` through `WorldInput`. Uncontextual gameplay messages and parallel `Move`/`Motion` forms are not accepted. Presentation clients negotiate `ContentOffer`/`ContentReady`; the offer carries
a required material manifest SHA-256 (zero selects base), separate HTTP port and
bounded climate scenario; the ready message echoes the hash. Desktop admission
waits for content readiness. Headless capabilities bypass material preparation.
The scenario is a validated binary record with at most 32 circular regions.

`ClientMessage`, `ServerMessage`, `WorldContext`, protocol limits and encode/decode functions are re-exported from `src/lib.rs`.

Movement samples carry a nonzero sequence and target simulation tick. Welcome
and snapshots report the server tick; snapshots also carry the active server
movement intent and a safe resolved-sequence prefix. The server accepts at most
eight ticks of future input, applies at most one sample per player per tick and
resolves late or rejected samples without replaying historical physics. The ack
cannot pass an earlier accepted sample that remains executable.

Protocol v19 also defines context-scoped entity synchronization begin, record,
complete and acknowledgement messages. Records encode spawn, component upsert,
component removal and despawn; component payloads are capped at the shared
2 KiB component limit before allocation and remain within the 4096-byte frame
limit. Sync IDs are nonzero; revision zero is valid for an initial full image
before any live entity mutation. Clients acknowledge only after application
commits the synchronization.
`InteractEntity` carries a world-scoped entity ID for gameplay interactions.

## Dependencies

Uses core domain identifiers and values. Domain packages do not depend on protocol.

## Data flow

Typed messages encode to bounded byte frames; input bytes decode to typed messages, incomplete input or a protocol error.

## Ownership and execution

Codec calls are synchronous and own no connection or mutable session state.

## Invariants and errors

Every wire version other than 19 is rejected; no compatibility decoder is provided. Preserve framing, field limits and explicit malformed-message errors.

## Commands

None.

## Change guide

Change wire representation here, then update all message producers/consumers and version documentation together.

## Verification

Run `rtk cargo check -p minimium-protocol` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.

Protocol v19 adds contextual SkyView and Bolid event frames with bounded paths and timing.
