# Game protocol v11

`minimium-protocol` defines typed messages and binary encoding without depending
on TCP, UDP, sockets or a runtime. The current server adapter carries frames over
TCP. The separate admin endpoint remains a local authenticated protocol.

## Frame format

Every frame begins with a four-byte header:

| Offset | Size | Meaning |
| --- | ---: | --- |
| 0 | 1 byte | Protocol version (`11`) |
| 1 | 1 byte | Message type |
| 2 | 2 bytes | Payload length, unsigned little-endian |

The payload follows immediately. Maximum payload is 4096 bytes. Receivers may
provide partial or concatenated frames; decoders report incomplete input
without consuming it. Unknown versions, message types, malformed fields and
oversized payloads are rejected. Integers and `f32` values are little-endian.
Movement and position values must be finite; names are 1–24 ASCII letters,
digits or underscores.

## World context

Each `WorldContext` is encoded as a 16-byte instance ID, 8-byte content revision
and 8-byte membership epoch. Gameplay input records and snapshots carry this
context. The server accepts an input only when all three values match the
player's current membership. The client discards snapshots from an old context.

A `WorldManifest` encodes a 16-byte definition ID, one-byte topology, 16-byte
seed, 4-byte generation revision, 4-byte position quantum, 8-byte content
revision and 32-byte generator digest, followed by a one-byte planet-definition
presence flag. A present definition adds mode, radius, height scale, sea level,
mountain limit, detail strength/wavelength, band frequency and a 32-byte GEBCO
package digest. The client verifies the map fingerprint
and generator digest in Welcome and transfer preparation before joining a world.

## Message types

The client sends Identify first with its 16-byte persistent profile ID. The
server sends Welcome only after accepting that profile.

| Direction | Type | Payload |
| --- | ---: | --- |
| Client → server | `1` Ping | empty |
| Client → server | `2` SetName | one-byte name length, then name |
| Client → server | `3` Move | horizontal input (`f32 x`, `f32 z`), jump flag |
| Client → server | `4` Quit | empty |
| Client → server | `5` Identify | 16-byte persistent profile ID; first message only |
| Client → server | `6` Fire | normalized world direction (`f32 x/y/z`) |
| Client → server | `7` Chat | one-byte UTF-8 byte length, then 1–128 text bytes |
| Client → server | `8` WorldInput | context, subtype, then SetName/Move/Fire/Chat/UsePortal/MapTeleport payload |
| Client → server | `9` TransitionReady | 16-byte transition ID and target context |
| Client → server | `10` UsePortal | empty; legacy unwrapped form is rejected by server admission |
| Client → server | `11` CancelTransition | 16-byte transition ID |
| Server → client | `0x81` Welcome | player ID, map fingerprint, health, restored position (`i64` millimeters x/y/z), context, manifest, optional transition ID |
| Server → client | `0x82` Pong | empty |
| Server → client | `0x83` Accepted | empty |
| Server → client | `0x84` Bye | empty |
| Server → client | `0x85` Error | one-byte error code |
| Server → client | `0x86` Snapshot | server time, context, self state, full/delta flag, player updates and removals |
| Server → client | `0x87` ShotResult | hit flag; on hit, target ID and remaining health; killed flag |
| Server → client | `0x88` HealthUpdate | local-player health (`u16`, 0–100) |
| Server → client | `0x89` Chat | sender ID/name and UTF-8 text |
| Server → client | `0x8a` TransitionPrepare | transition ID, source/target contexts, fingerprint, destination, target manifest |
| Server → client | `0x8b` TransitionCommitted | transition ID, target context, fingerprint, destination, health, target manifest |
| Server → client | `0x8c` TransitionCancelled | transition ID and source context |
| Server → client | `0x8d` MapTeleportResult | success flag; on success, destination (`i64` millimeters x/y/z) |

For `WorldInput`, subtypes are `1` SetName, `2` Move, `3` Fire, `4` Chat and
`5` UsePortal and `6` MapTeleport (unit direction, three `f32` values). The client session wraps ordinary gameplay messages in this record
using its current context. `TransitionReady` is sent only after the client
validates the target manifest. The server commits ready transfers at a tick
boundary and forces a full snapshot in the new context. A cancellation succeeds
only before that commit.

Snapshots begin with monotonic server time (`u64`), the 32-byte context, local
position (three `i64` millimeters) and velocity (three `f32` values), a grounded flag, full/delta
flag, and one-byte update/removal counts. Each update contains player ID, name
and position (`i64` millimeters per axis); each removal contains a player ID. Both lists are bounded to 16.
The server advances a per-client baseline only after queuing a snapshot
successfully.
Portal transition destinations also use three `i64` millimeter coordinates.

## Runtime policy and persistence

Fire uses authoritative server-side hitscan with 60-unit range and 34 damage.
Chat is bounded UTF-8, rate-limited per sender and globally. Interactive clients
send Ping every 10 seconds; server idle timeout is 30 seconds and the desktop
client's Pong timeout is 20 seconds. These are runtime policies, not frame
requirements.

Save format v7 records every loaded world's manifest and map fingerprint, plus
each player's instance, quantized position, orientation, health and return
location. v2/v3 snapshots are migrated only to a loaded legacy map whose
fingerprint matches the saved header. The intermediate v4 format is accepted
only when player and return locations use the primary instance because it did
not pin target manifests. This prevents old coordinates from being assigned to
a new procedural planet. Snapshots are limited to 16 MiB; an oversized write is
rejected before replacing the previous snapshot.

The current TCP adapter has no public-game authentication or encryption. The
profile ID is a reconnect identifier, not proof of account ownership. Gameplay,
transfer, migration and reconnect have workspace coverage. A wire-level v9
demonstration also completed planet → dungeon → planet and restored the profile
after a graceful server restart. These results do not verify the rendered GPU
scene; that transfer still needs visual review.
