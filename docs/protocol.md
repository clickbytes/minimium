# Game protocol v4

The `minimium-protocol` crate defines typed game messages and binary encoding without depending on TCP, UDP, sockets, or a runtime. The current server adapter carries these frames over TCP. The separate admin endpoint remains a local authenticated text protocol.

## Frame format

Every frame begins with a four-byte header:

| Offset | Size | Meaning |
| --- | ---: | --- |
| 0 | 1 byte | Protocol version (`4`) |
| 1 | 1 byte | Message type |
| 2 | 2 bytes | Payload length, unsigned little-endian |

The payload follows immediately. The maximum payload is 4096 bytes. TCP receivers may provide partial frames or several concatenated frames; decoders report incomplete input without consuming it. Unknown versions, unknown message types, malformed fields, and oversized payloads are rejected.

Integers and IEEE-754 `f32` values use little-endian encoding. Movement and position values must be finite. Player names are 1–24 ASCII letters, digits, or underscores.

## Message types

Before sending any other game message, a client must send message type `5` (`Identify`) with its 16-byte profile ID. The server sends `Welcome` only after the profile is accepted.

| Direction | Type | Payload |
| --- | ---: | --- |
| Client -> server | `5` Identify | 16-byte persistent client profile ID; must be the first message |
| Client → server | `1` Ping | empty |
| Client → server | `2` SetName | one-byte name length, then name bytes |
| Client → server | `3` Move | horizontal direction (`f32 x`, `f32 z` in `-1..=1`), jump flag (`u8` 0 or 1) |
| Client → server | `4` Quit | empty |
| Server → client | `0x81` Welcome | player ID (`u64`), canonical map fingerprint (`u64`) |
| Server → client | `0x82` Pong | empty |
| Server → client | `0x83` Accepted | empty |
| Server → client | `0x84` Bye | empty |
| Server → client | `0x85` Error | one-byte error code |
| Server → client | `0x86` Snapshot | self position, self velocity, grounded flag, count, then nearby player records |

A snapshot contains the local position and velocity as three `f32` values each, a one-byte grounded flag, a one-byte player count, and up to 16 records. Each record contains player ID (`u64`), one-byte name length and name, then three position values. Movement messages carry normalized horizontal input; jump is an edge-triggered flag. The server integrates gravity, collision and movement at its fixed 20 Hz tick.

The welcome fingerprint is calculated from parsed map records, not source file bytes, so line-ending and whitespace changes do not alter it. The client disconnects when its map fingerprint differs from the server's. This detects accidental map mismatch; it is not an authentication mechanism.

## Current limits

Version 4 carries complete nearby snapshots rather than delta replication. TCP currently provides ordered reliable delivery for every message, including replaceable snapshots. There is no public-game authentication or encryption. These constraints make the current adapter suitable for local prototyping only; they do not define the future production transport.

The profile ID is generated and stored in the client's data directory. It lets this prototype reconnect a local client to its saved character. It is a public identifier, not proof of account ownership; clients can spoof it until real authentication is implemented.
