# minimium-transport

## Purpose / Non-goals

Owns host-side game and administrative TCP listeners and the client TCP adapter, sockets, bounded byte queues, framing/codecs, partial writes, and stable transport IDs. It exposes typed events and bounded message ports. It does not own gameplay authentication, principal permissions, sessions, commands, world rules, or persistence.

## Public API

The process host calls `HostTransport::bind` and supplies it through the `TransportPort` interface. The runtime polls bounded `TransportEvent` batches and obtains a cloneable `GameOutputPort` for typed `ServerMessage` output. `ConnectionId` and `AdminRequestId` are checked monotone identifiers; exhaustion is explicit and IDs are not reused. A successful enqueue means the adapter accepted the response, not that the peer received it.

The neutral port can be implemented by a non-TCP host. Socket addresses and binding are available only on the concrete host adapter; runtime code depends on `TransportPort` and `GameOutputPort`.

The client owner supplies `client::ClientTransportPort` to its logical session.
`TcpClientTransport::new` configures an already connected stream; this is the sole
TCP implementation used by desktop and terminal. The client port exposes bounded
read progress, typed front-message peek/consume, typed output admission, flush
progress, cumulative byte metrics, peer-address metadata and immediate close.
No socket or mutable byte buffer escapes the port. A peek retains its raw frame's
capacity charge until consumption, allowing logical transition deferral without
unbounded decoded queues.

`ClientFrameToken` identifies one accepted frame for the transport lifetime.
The TCP adapter uses checked cumulative accepted-byte ends, independent of buffer
compaction; tokens never wrap or repeat. `flush(Some(token), now)` accepts a
host-provided monotonic `Instant` and reports that sample only when a successful
write completes the tracked frame. It does not read a hidden clock. The pure
`validate_time(now)` guard and flush reject backward samples before socket I/O,
buffer accounting or token removal; equality is valid. This neutral standard
library argument keeps logical runtime clock types out of the transport crate.
An accepted queue entry and a server response are separate outcomes. The client
runtime uses this completion for Ping RTT/timeout; other session policy stays out
of the adapter. Alternate ports implement the same ownership and boundedness.

## Dependencies

Uses the standard library and `minimium-protocol` for game framing and message codecs. It has no dependency on server runtime or gameplay. Runtime policy and command parsing remain outside this crate.

## Data flow

The process host binds listeners and creates `HostTransport`. The adapter owns each accepted socket, input buffer, decoded frame progression, output buffer, partial-write offset and close-after-flush deadline. It emits game connect/message/disconnect events keyed by stable `ConnectionId`; admin AUTH lines become bounded command-line request events keyed by `AdminRequestId`. The runtime retains principal authentication/admission and command interpretation. It sends typed game messages and neutral admin response data back through the ports.

Game output preserves FIFO order, the 64 KiB/256-frame per-connection queue bounds, 1 KiB per-client read budget, 16 KiB per-client and 256 KiB total game write budgets, and bounded poll event batches. Admin transport preserves the 8-client cap, 4 accepts per poll, 512-byte request line, 2-second request/response deadlines, 1 KiB read and 4 KiB write budgets, and 64 KiB response cap. Partial writes and `WouldBlock` retain pending bytes and offsets; closed or stale connection IDs reject enqueue without creating orphaned queue entries.

Client TCP preserves its distinct 64 KiB receive/send bounds and 16 KiB read
budget per session poll, reading in at most 4096-byte chunks. Each read fits
the remaining receive capacity. When content preparation defers the front
message and the buffer fills, polling leaves further bytes in TCP until the
session consumes buffered frames. Flush writes until
blocked or the bounded queue empties; there is no new write quantum. The existing
protocol codec handles its 4100-byte maximum frame. EOF retains complete messages
for consumption before the logical owner closes; incomplete trailing data is
discarded at close. Other I/O errors and codec/receive-limit errors are typed.
Output backpressure, closed adapter, encoding failure and token exhaustion are
distinct queue errors. Immediate close/drop discards pending output; it does not
claim delivery or add a graceful-drain phase.

The admin adapter compares the configured token and handles line framing and JSON error envelopes. The runtime parses commands, checks command policy, executes them, and renders typed replies into bounded neutral responses. The game adapter validates framing and protocol versions; the runtime decides whether a connected peer is admitted and what its principal may do.

## Ownership and execution

The adapter polls synchronously on the runtime owner lane. The process host owns construction and lifecycle wiring; the adapter owns transport resources; the runtime owns gameplay admission and response meaning.

## Invariants and errors

Listener/socket failures and identifier exhaustion return typed transport errors. Nonzero `ConnectionId` and `AdminRequestId` constructors let alternate hosts create stable IDs; each host must allocate IDs without reuse. Malformed game frames produce a disconnect event; unsupported protocol versions enqueue the existing protocol error and close after flush. Backpressure, unknown game IDs, unknown or expired admin requests, oversized admin replies, closed IDs and encoding failure are distinct output errors. Oversized replies queue a bounded fallback while returning a typed rejection to the caller. Event and output resources remain bounded even if the runtime does not drain events promptly. Shutdown closes admission first, lets runtime reject outstanding admin requests, drains only admin responses to the supplied deadline, then closes game and admin sockets. Dropping the TCP host also invalidates game output handles and closes all owned sockets.

## Verification

Run `rtk cargo check -p minimium-transport -p minimium-server-runtime -p minimium-server --locked`, architecture and file-length checks. Runtime acceptance scenarios remain deferred.

## Commands

None.

## Change guide

Change framing, bounded socket I/O and transport lifecycle here. Change authentication policy, command parsing or gameplay permissions in their runtime/application owners.
