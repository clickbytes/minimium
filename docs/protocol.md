# Game protocol v8

The `minimium-protocol` crate defines typed game messages and binary encoding without depending on TCP, UDP, sockets, or a runtime. The current server adapter carries these frames over TCP. The separate admin endpoint remains a local authenticated text protocol.

## Frame format

Every frame begins with a four-byte header:

| Offset | Size | Meaning |
| --- | ---: | --- |
| 0 | 1 byte | Protocol version (`8`) |
| 1 | 1 byte | Message type |
| 2 | 2 bytes | Payload length, unsigned little-endian |

The payload follows immediately. The maximum payload is 4096 bytes. TCP receivers may provide partial frames or several concatenated frames; decoders report incomplete input without consuming it. Unknown versions, unknown message types, malformed fields, and oversized payloads are rejected.

Integers and IEEE-754 `f32` values use little-endian encoding. Movement and position values must be finite. Player names are 1–24 ASCII letters, digits, or underscores.

## Message types

Before sending any other game message, a client must send message type `5` (`Identify`) with its 16-byte profile ID. The server sends `Welcome` only after the profile is accepted.

For `SetName`, `Accepted` acknowledges admission to the ordered world-input queue, not completion of the later world mutation or persistence. An output queue may drop this acknowledgement even when the input was admitted.

| Direction | Type | Payload |
| --- | ---: | --- |
| Client -> server | `5` Identify | 16-byte persistent client profile ID; must be the first message |
| Client → server | `1` Ping | empty |
| Client → server | `2` SetName | one-byte name length, then name bytes |
| Client → server | `3` Move | horizontal direction (`f32 x`, `f32 z` in `-1..=1`), jump flag (`u8` 0 or 1) |
| Client → server | `4` Quit | empty |
| Client → server | `6` Fire | normalized world direction (`f32 x`, `f32 y`, `f32 z`) |
| Client → server | `7` Chat | one-byte UTF-8 byte length, then `1..=128` bytes of text without control characters |
| Server → client | `0x81` Welcome | player ID (`u64`), canonical map fingerprint (`u64`), restored health (`u16`, `0..=100`) |
| Server → client | `0x82` Pong | empty |
| Server → client | `0x83` Accepted | empty |
| Server → client | `0x84` Bye | empty |
| Server → client | `0x85` Error | one-byte error code |
| Server → client | `0x86` Snapshot | server time, self state, full/delta flag, player update count, removal count, updates, then removed player IDs |
| Server → client | `0x87` ShotResult | hit flag; on hit, target ID and remaining health; then killed flag |
| Server → client | `0x88` HealthUpdate | current local-player health (`u16`, `0..=100`) |
| Server → client | `0x89` Chat | sender ID (`u64`), one-byte sender name length and name, then one-byte UTF-8 text length and text |

Error payload codes are one byte. Unknown values are preserved by the typed API and can be encoded again unchanged.

| Error | Byte |
| --- | ---: |
| Protocol | `1` |
| Busy | `2` |
| ServerFull | `3` |
| RateLimit | `4` |
| Unknown | Any other byte |

Player and profile IDs use domain types in the API; their wire representations remain `u64` and 16 bytes respectively. The historical ShotResult encoder accepts any `u16` remaining health, while the decoder rejects values above 100; this asymmetry is preserved for v8 compatibility.

A snapshot begins with the server's monotonic time in milliseconds since process start (`u64`), the local position and velocity as three `f32` values each, and a one-byte grounded flag. A one-byte `full snapshot` flag is followed by one-byte player update and removal counts. A full snapshot replaces the client's remote-player set; later snapshots upsert changed records and remove the listed player IDs. Each update contains player ID (`u64`), one-byte name length and name, then three position values. Each removal contains a player ID (`u64`). Both lists are bounded to 16 entries. The server sends a full snapshot first for each connection. It advances its per-client baseline only after successfully queuing a snapshot, so a snapshot dropped by the output-queue limit is included in a later delta. Movement messages carry normalized horizontal input; jump is an edge-triggered flag. The server integrates gravity, collision and movement at its fixed 20 Hz tick. The client uses the server timestamp and local elapsed receive time to interpolate remote players with a 100 ms presentation delay.

Fire uses an authoritative hitscan query from the attacker's eye, with a 60-unit range and 34 damage. Solid boxes and triangles block shots; the nearest visible player hit takes damage. Players have 100 health and respawn at the map spawn after health reaches zero. The server rate-limits fire to one shot every 250 ms. It sends shot results to the shooter and coalesces health updates for the victim. Player health is stored in world snapshot version 3 and restored when the profile reconnects. Version 2 snapshots migrate with full health. Manual saves capture current health; controlled shutdown writes a final save, while a crash can lose changes since the most recent save.

Chat messages are validated as non-empty UTF-8 text of at most 128 bytes with no control characters. The server broadcasts each accepted message to connected players, rate-limits each sender to one message per second, and caps the global broadcast rate at 20 messages per second. Broadcasts reuse one encoded frame and append it to each client's bounded output queue; a slow client's full queue can drop a chat message without blocking the simulation.

The welcome fingerprint is calculated from parsed map records, not source file bytes, so line-ending and whitespace changes do not alter it. The client disconnects when its map fingerprint differs from the server's. This detects accidental map mismatch; it is not an authentication mechanism.

## Current limits

Version 8 carries bounded deltas for nearby players while sending the local player's authoritative state in every snapshot, hitscan combat messages, and global text chat. TCP provides ordered reliable delivery. Queued deltas depend on the previously queued baseline and cannot be reordered or coalesced; the server may skip building a delta when the bounded output queue cannot accept it, without advancing that baseline. There is no public-game authentication or encryption. These constraints make the current adapter suitable for local prototyping only; they do not define the future production transport.

Interactive clients send an empty `Ping` every 10 seconds while connected. The desktop client expects a `Pong` within 20 seconds and exposes the latest round-trip time through `net stats`; the server drops a connection after 30 seconds without a valid game message. The server allows up to 512 active players and 64 unidentified connections; each new connection must send `Identify` within five seconds. It accepts at most 64 new sockets per poll. Admin requests are local-only, with a 512-byte line limit and a two-second total request deadline. These are current runtime policies; the frame format does not require a specific heartbeat cadence.

The server tracks awaiting Identify, active identity, and draining close states. An EOF observed while reading is recorded until complete frames already decoded in that poll are dispatched, then the connection drains its queued responses for up to two seconds. Per poll it reads at most 1024 bytes and decodes at most 32 frames per client; output is bounded to 64 KiB and 256 frames per client. Sending uses at most 16 KiB per client and 256 KiB in total per iteration, rotating the first client. A partially written frame retains its remaining bytes and accounting. A replication baseline advances only after its frame is queued successfully. These are implementation contracts to verify in the deferred network scenarios, not measured results.

The profile ID is generated and stored in the client's data directory. It lets this prototype reconnect a local client to its saved character. It is a public identifier, not proof of account ownership; clients can spoof it until real authentication is implemented.
