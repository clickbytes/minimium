# minimium-storage

## Purpose / Non-goals

Owns snapshot serialization and validation, player persistence records and the bounded persistence worker. It does not own live runtime worlds or network transport.

## Public API

`PlayerStore` loads and updates persistent player state. The server captures immutable `WorldSaveInput` values before handing them to `PersistenceWorker`; the worker writes `SaveSnapshot` values.

## Dependencies

Consumes core identity, world and simulation values, environment clock state, the bounded diagnostics contract, telemetry and platform file operations. The player-name byte limit belongs to core identity; storage does not depend on network protocol or telemetry file sinks.

## Data flow

The owning server lane builds a snapshot input from world state. `PlayerStore` updates its profile shards and captures immutable `SaveSnapshot` values; the persistence worker queues and writes those snapshots. The format adapter parses the header, then validates clock, world, sector, entity, component, initialization and player records in order, or serializes the same records to a temporary file before atomic replacement.

The current snapshot limit is 128 MiB and 16,384 saved world instances. The encoder first runs against a counting writer, then streams the same encoding to a temporary file through a size-limited writer before atomic replacement. Each world has an explicit entity counter and content-initialization marker; entities and components use independent records, with component bytes hex-encoded and limited to 2048 bytes after decoding. Loading validates duplicate IDs, world references, exact registered component schemas, component counts and the 4 MiB aggregate component-payload limit before publishing a `LoadedState`. `SnapshotBudget` reserves each profile's maximum legal encoded record before a new profile is inserted and rebuilds those reservations while loading; profile reservations are capped at 64 MiB. Its fixed metadata reserve is derived from a 1024-byte per-world cap (checked by counting the actual manifest/counter/initialization encoders) across 16,384 worlds, plus 64 KiB for the primary header and clock. Sector, entity and component record sizes are measured with their format encoders and reserved by stable world/record key. Component raw-payload reservations are tracked alongside encoded record reservations and released on removal. The global budget cannot exceed 128 MiB. Multi-record reservations stage against a cloned budget and commit only if every reservation fits.

## Ownership and execution

The server owns `PlayerStore` and captures live player and world data on its owner lane. Snapshot capture keeps the public `PlayerStore::save_snapshot` entry point, assigns a monotone process-local revision, and copies the current full entity set so despawned entities cannot return from stale load state. The worker rejects stale revisions; shutdown queues the final higher revision after any earlier autosave and waits for it before joining. Timeout/error is propagated to the server process status and does not claim cancellation of blocked OS I/O; writes use temporary files and atomic replacement so a failed pre-replacement write preserves the prior complete snapshot. The worker owns its bounded request channel, save-status bookkeeping, queued immutable snapshots and configured data directory.

## Invariants and errors

Snapshot loading, capture, writing and persistence-worker operations return `StorageError`, with stable codes for invalid formats, unsupported versions, size limits, busy queues and stopped workers. The original I/O error remains available through `Error::source`; the server may adapt it at its process boundary. Missing snapshots still represent an empty store.

## Commands

No commands. The server runtime owns `world save` dispatch.

## Change guide

Change snapshot format and validation here. Build runtime `WorldSaveInput` values in the server before crossing the worker boundary.

## Verification

Run `rtk cargo check -p minimium-storage -p minimium-server` and architecture/file-length checks. Runtime acceptance is deferred.

Save format v10 is the only accepted snapshot format. Older and unknown versions
are rejected before startup creates a persistence worker or replaces a file;
there is no migration path. Use a separate data directory when creating a v10
world from older data. A v5 planet manifest stores peak start/full metres,
multiplier permille and exponent after its package digest. Parsing is gated by
the manifest's generator revision, including embedded manifests; other
generated revisions are rejected rather than reinterpreted. This is a generator
contract, independent of save v10 and protocol v18.
