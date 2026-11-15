# minimium-transport

## Purpose / Non-goals

Owns the host-side game and administrative TCP listeners, sockets, bounded byte queues, framing/codecs, partial writes, and transport connection/request IDs. It exposes typed events and bounded typed output ports. It does not own gameplay authentication, principal permissions, sessions, commands, world rules, or persistence.

## Public API

The process host calls `HostTransport::bind` and supplies it through the `TransportPort` interface. The runtime polls bounded `TransportEvent` batches and obtains a cloneable `GameOutputPort` for typed `ServerMessage` output. `ConnectionId` and `AdminRequestId` are checked monotone identifiers; exhaustion is explicit and IDs are not reused. A successful enqueue means the adapter accepted the response, not that the peer received it.

The neutral port can be implemented by a non-TCP host. Socket addresses and binding are available only on the concrete host adapter; runtime code depends on `TransportPort` and `GameOutputPort`.

## Dependencies

Uses the standard library and `minimium-protocol` for game framing and message codecs. It has no dependency on server runtime or gameplay. Runtime policy and command parsing remain outside this crate.

## Data flow

The process host binds listeners and creates `HostTransport`. The adapter owns each accepted socket, input buffer, decoded frame progression, output buffer, partial-write offset and close-after-flush deadline. It emits game connect/message/disconnect events keyed by stable `ConnectionId`; admin AUTH lines become bounded command-line request events keyed by `AdminRequestId`. The runtime retains principal authentication/admission and command interpretation. It sends typed game messages and neutral admin response data back through the ports.

Game output preserves FIFO order, the 64 KiB/256-frame per-connection queue bounds, 1 KiB per-client read budget, 16 KiB per-client and 256 KiB total game write budgets, and bounded poll event batches. Admin transport preserves the 8-client cap, 4 accepts per poll, 512-byte request line, 2-second request/response deadlines, 1 KiB read and 4 KiB write budgets, and 64 KiB response cap. Partial writes and `WouldBlock` retain pending bytes and offsets; closed or stale connection IDs reject enqueue without creating orphaned queue entries.

The admin adapter compares the configured token and handles line framing and JSON error envelopes. The runtime parses commands, checks command policy, executes them, and renders typed replies into bounded neutral responses. The game adapter validates framing and protocol versions; the runtime decides whether a connected peer is admitted and what its principal may do.

## Ownership and execution

The adapter polls synchronously on the runtime owner lane. The process host owns construction and lifecycle wiring; the adapter owns transport resources; the runtime owns gameplay admission and response meaning.

## Invariants and errors

Listener/socket failures and identifier exhaustion return typed transport errors. Malformed game frames produce a disconnect event; unsupported protocol versions enqueue the existing protocol error and close after flush. Backpressure, unknown IDs, closed IDs and encoding failure are distinct output errors. Event and output resources remain bounded even if the runtime does not drain events promptly. Shutdown closes admission first, lets runtime reject outstanding admin requests, drains queued responses to the supplied deadline, then closes sockets.

## Verification

Run `rtk cargo check -p minimium-transport -p minimium-server-runtime -p minimium-server --locked`, architecture and file-length checks. Runtime acceptance scenarios remain deferred.

## Commands

None.

## Change guide

Change framing, bounded socket I/O and transport lifecycle here. Change authentication policy, command parsing or gameplay permissions in their runtime/application owners.
