# minimium-protocol

## Purpose / Non-goals

Owns versioned network message models, byte codecs, validation and frame limits. TCP buffering and gameplay decisions belong outside this crate.

## Public API

The only supported protocol version is 22. It uses a bounded provider credential message and requires every gameplay action to carry an explicit `WorldContext` through `WorldInput`. Uncontextual gameplay messages and parallel `Move`/`Motion` forms are not accepted. Presentation clients negotiate `ContentOffer`/`ContentReady`; the offer carries
a required material manifest SHA-256 (zero selects base), separate HTTP port and
bounded climate scenario; the ready message echoes the hash. Desktop admission
waits for content readiness. Headless capabilities bypass material preparation.
The in-development v22 tool-result codec reserves rejection tag 11 for an unsafe
geometry join, separately from a blocking first hit (tag 4). Rebuild both endpoints
when updating this codec; the matching save format is v12.
The scenario is a validated binary record with at most 32 circular regions.

`ClientMessage`, `ServerMessage`, `WorldContext`, protocol limits, encode/decode
functions and the shared `is_unit_direction`/`valid_chat` content validators are
re-exported from `src/lib.rs`. The validators expose existing codec rules for
owner-side checks without changing encoded frames or protocol version.

World inputs carry a connection sequence, control revision and world context. Movement has a separate sequence and advisory target tick. The server coalesces held intentions at the next available step and acknowledges both a resolved sequence prefix and the last applied sequence/tick pair. Pause produces a typed rejection.

Welcome, environment sync and transfer carry session nonce, control revision, calendar epoch, committed tick, UTC, requested and achieved rates, limiting reason and a monotonic observation timestamp. Elapsed game milliseconds equal 50 times the tick. A snapshot's elapsed time must match its tick. Sky observations include observer position, epoch, observed UTC and a bounded confirmed night interval. Bolid starts and impact samples use simulation milliseconds. RTT probes use host monotonic time.

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

Every wire version other than 26 is rejected; no compatibility decoder is provided. Preserve framing, field limits and explicit malformed-message errors.

## Commands

None.

## Change guide

Change wire representation here, then update all message producers/consumers and version documentation together.

## Verification

Run `rtk cargo check -p minimium-protocol` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.

Protocol v19 adds contextual SkyView and Bolid event frames with bounded paths and timing.

## Protocol 20 impact stream

`ImpactSync` (0x98) brackets the initial durable body image. `ImpactState` (0x97)
carries WorldContext, server time, live-presentation flag and one validated
162-byte BodyState. This stream was introduced in v20 and remains part of v22; bolids carry a simulation-time origin. The initial image has a stable ID list, at most 16 records per
server tick and no historical explosion replay. Live changes follow over ordered
TCP. A new membership receives a new full image. The stream covers the current
world, bounded to 4096 bodies; it is not an unbounded global event history.

## Protocol 21 excavation stream

Excavation inputs and results carry `WorldContext`. Chunk images and independent
crater stamps use bounded
multipart records inside atomic batches, with a 4000-byte part cap. Clients stage
complete images before publication; a revision gap requests resynchronization.
Coherent fluid snapshots carry both local cells and reservoir balances in bounded
parts and publish only after the complete image validates.
Reservoir quantities include their compensated volume residuals. Cells are a
replacement image for the announced region; the paired reservoir balances remain
complete. A structural batch installs its terrain and captured water together.

Wire v23 adds single-field movement patches with optional policy revision checks and correlated PolicyReply messages. Speed and acceleration use metres per second and metres per second squared. The sequence and world context travel back to the requesting session.

Wire v24 adds an explicit jump request and an intent-tagged MovementReceipt for admission, final physical resolution, coalescing or rejection. Ordinary device motion keeps its existing bounded stream.

Wire v25 adds a correlated PlayerActionReceipt for move, chat, name, fire and tool grants. Acceptance means the owner admitted the request; commitment follows execution or durable inventory publication. A rejection carries a reason code: 1 permission, 2 busy, 3 invalid, 4 rate limit, 5 unavailable, 6 stale world, 7 paused, 8 superseded. Disconnection leaves any unconfirmed request outcome unknown.

Wire v26 extends the correlated receipt with portal interaction, geographic goto and altitude action kinds. Portal preparation carries the matching transition ID. Reason 9 means an Earth world is required, 10 means flight is required, and 11 means another transfer is pending. The established TransitionPrepare, TransitionReady, TransitionCommitted and TransitionCancelled messages still own transfer content and membership changes; a portal receipt follows their final owner decision.

Client operation binding and the wire codec share valid_name and valid_chat. The name predicate accepts 1 through 24 ASCII letters, digits or underscores. Chat accepts 1 through 128 UTF-8 bytes and rejects control characters.
