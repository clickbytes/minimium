# minimium-storage

## Purpose / Non-goals

Owns snapshot serialization and validation, player persistence records, bounded durable workers and public backend/service ports. It does not own live runtime worlds or network transport.

## Public API

`MemoryStorageBackend::new(MemoryStorageLimits)` implements the same snapshot,
world-change and impact ports without files or worker threads. Clones retain one
volatile committed image; a single lease covers all opened services. Loading or
opening a second service group while the first is alive returns Busy. Drop all
ports before reopening. Snapshot capture/encoding/loading use the existing codec
and budget; queued acceptance becomes completion only when the port is polled.
Memory acknowledgements promise retention in this backend, not survival of the
process. An explicit shutdown drains pending operations under its deadline.

Limits may lower the existing 128 MiB snapshot, 128 MiB excavation checkpoint,
32 MiB excavation transaction and 4096-impact maxima. Pending capacity is one
snapshot, eight excavation mutations and sixteen one/two-body impact batches.
The previous committed snapshot is retained while a replacement is queued;
validation/staging copies are bounded by these same caps. Rejection preserves
the prior committed image. `excavation::validate_restore` applies byte/count and
identity preflight plus the existing checkpoint decoder to raw injected restores.
The runtime independently validates schemas/world identities and acquires neutral
entity-change storage reservations before publishing its constructed owner.

`PlayerStore` loads and updates persistent player state. The server captures immutable `WorldSaveInput` values and `SaveSnapshot` values before handing snapshots to `PersistencePort`; the port's queue acceptance is not a durable acknowledgement. `StorageBackend` exposes preflight, load and open-service operations. `FileStorageBackend` is the explicit path-owning implementation used by the standalone process; a non-file host can implement the same ports without supplying a data directory.

## Dependencies

Consumes core identity, world and simulation values, gameplay-owned world-change plans, environment clock state, the bounded diagnostics contract, telemetry and platform file operations. The player-name byte limit belongs to core identity; storage does not depend on network protocol or telemetry file sinks.

## Data flow

The owning server lane builds a snapshot input from world state. `PlayerStore` updates its profile shards and captures immutable `SaveSnapshot` values; `PersistencePort` accepts or rejects bounded enqueue and later exposes typed completion/status. The file backend parses the header, then validates clock, world, sector, entity, component, initialization and player records in order, or serializes the same records to a temporary file before atomic replacement. The world-change port consumes gameplay-owned plans and separates typed validation, queue acceptance and exact-envelope acknowledgement. File codec DTOs and encoding stay private to storage; restore returns neutral materialized state. Impact journal behavior remains unchanged.

World-change failures carry `IdentityConflict`, `RevisionConflict`, `LimitExceeded`, `InvalidPlan`, `Busy`, `Unavailable` or `Failure`; technical I/O errors remain available as the failure source. `poll_acknowledged` reports a typed commitment: `Durable` for the file backend and `Volatile` for memory.

The current snapshot limit is 128 MiB and 16,384 saved world instances. The encoder first runs against a counting writer, then streams the same encoding to a temporary file through a size-limited writer before atomic replacement. Each world has an explicit entity counter and content-initialization marker; entities and components use independent records, with component bytes hex-encoded and limited to 2048 bytes after decoding. Loading validates duplicate IDs, world references, exact registered component schemas, component counts and the 4 MiB aggregate component-payload limit before publishing a `LoadedState`. `SnapshotBudget` reserves each profile's maximum legal encoded record before a new profile is inserted and rebuilds those reservations while loading; profile reservations are capped at 64 MiB. Its fixed metadata reserve is derived from a 1024-byte per-world cap (checked by counting the actual manifest/counter/initialization encoders) across 16,384 worlds, plus 64 KiB for the primary header and clock. Sector, entity and component record sizes are measured with their format encoders and reserved by stable world/record key. Component raw-payload reservations are tracked alongside encoded record reservations and released on removal. Ordered `WorldEntityChange` batches are measured by the entity/component record encoders and prepared against a cloned budget; the prepared reservation holds an exclusive budget borrow until commit or drop, preventing an intervening budget change from making it stale. The global budget cannot exceed 128 MiB. Multi-record reservations stage against a cloned budget and commit only if every reservation fits.

## Ownership and execution

The server owns `PlayerStore` and captures live player and world data on its owner lane. Snapshot capture keeps the public `PlayerStore::save_snapshot` entry point, assigns a monotone process-local revision, and copies the current full entity set so despawned entities cannot return from stale load state. The worker rejects stale revisions; shutdown queues the final higher revision after any earlier autosave and waits for it before joining. Timeout/error is propagated to the server process status and does not claim cancellation of blocked OS I/O; writes use temporary files and atomic replacement so a failed pre-replacement write preserves the prior complete snapshot. The worker owns its bounded request channel, save-status bookkeeping, queued immutable snapshots and configured data directory.

`PersistenceWorker::status` returns a read-only `PersistenceStatus` snapshot containing the pending-save count and typed last-save state. It exposes no worker or filesystem handles; presentation adapters format this immutable data.

`PersistencePort::queue_save` accepts an already captured immutable `SaveSnapshot` and runtime-scoped `SaveJobId`; it never receives mutable `PlayerStore` access. A successful enqueue reports acceptance only. `poll_completions` returns matching job ID, revision, result and durability. A stopped worker emits failure for each still-pending ID. Busy refuses the new request without changing the older pending result. The file backend reports `Durable` after atomic write completion; memory reports `Volatile` after retention; injected ports default to `BackendDefined`. Completion and shutdown failure remain observable through typed status/completion methods. Journal ports return exact mutation/body acknowledgements after committing to the selected backend. `FileStorageBackend` keeps the current snapshot and journal codecs, journal lock, bounded worker queues, replay and deadlines behind these contracts. The standalone process separately owns the enclosing world-data lock.

## Invariants and errors

Snapshot loading, capture, writing and persistence-worker operations return `StorageError`, with stable codes for invalid formats, unsupported versions, size limits, busy queues and stopped workers. The original I/O error remains available through `Error::source`; the server may adapt it at its process boundary. Missing snapshots still represent an empty store.

The snapshot adapter maps core `ProfileIdParseError` to `InvalidData` (public
`StorageErrorKind::InvalidFormat`), preserving the parse detail and actual
`world.snapshot` path. Profile-ID alphabet, length and snapshot encoding are
unchanged; a rejected player record never publishes the staged loaded store.

The file excavation journal is stored in `excavation.v2` beside `world.snapshot`; back
up and restore both files with the complete world directory. It fsyncs a result
record before acknowledging it. Checkpoint compaction runs after that durable
commit; a compaction error does not revoke the already durable result, but the
journal can grow until a later successful compaction or the configured byte
budget rejects further mutations.
Monitor journal size and surface compaction failures when operational telemetry
is added.

## Commands

No commands. The server runtime owns `world save` dispatch.

## Change guide

Change snapshot format and validation here. Build runtime `WorldSaveInput` values in the server before crossing the worker boundary.

## Verification

Run `rtk cargo check -p minimium-storage -p minimium-server` and architecture/file-length checks. Runtime acceptance is deferred.

Save format v12 is the only accepted snapshot format. Older and unknown versions
are rejected before startup creates a persistence worker or replaces a file;
there is no migration path. Use a separate data directory when creating a v12
world from older data. A v6 planet manifest stores peak start/full metres,
multiplier permille and exponent after its package digest. Parsing is gated by
the manifest's generator revision, including embedded manifests; other
generated revisions are rejected rather than reinterpreted. This is a generator
contract, independent of save v12 and protocol v22.

## Excavation journal

`excavation::ExcavationJournal` owns the versioned append journal and atomic
checkpoint beside the world data. It persists resulting immutable chunk images,
profile-keyed inventories, and optional coherent fluid snapshots. Mutations are
revision checked before append; the bounded worker acknowledges only after
`sync_all`. Complete damaged records fail closed, while only an incomplete final
record is truncated under an exclusive file lock. The default transaction cap is
32 MiB, request queue capacity is eight, and the journal byte budget defaults to
128 MiB; `JournalConfig` can lower those limits. Replay restores latest chunk,
fluid, inventory, seed and mutation state without replaying domain commands.
Replay explicitly rewinds the append handle before reading, including immediately
after creating a new journal header.
It also stores durable impact body states, independent crater stamps and the
water-catalog digest for each world.
Fluid records preserve reservoir volume residuals with their primary quantities,
so replay retains small exchanges from large finite basins. Record byte budgets
include these fields in both transactions and compacted checkpoints.

## Impact journal

`impact_journal::ImpactJournal` owns `impacts.v2` beside `world.snapshot`.
Its version-2 fixed records contain a count, up to two 162-byte bodies and a
checksum. Parent/daughter creation is one record. Structural landing results,
craters and displaced water commit through the excavation journal; flight
checkpoints remain in this journal. A worker acknowledges only after `sync_all`; no filesystem
work runs in the simulation tick. Requests and completions each have capacity 16.
Replay rejects invalid identity/phase transitions and damaged complete records;
only an incomplete trailing record is truncated. At 4 MiB, atomic compaction
retains the latest state of all bodies, including collected crater owners.
The log has an 8 MiB read bound and 4096-body cap. Shutdown errors/timeouts propagate.
Save format 12 also stores the body component, while the two journals restore
their authoritative flight and structural results. Back up/restore the entire
world directory, including `impacts.v2` and `excavation.v2`.
Flight checkpoints occur at spawn, fragmentation, five-second boundaries and
shutdown; a process crash may rewind an unfinished flight to its last checkpoint.
Committed craters never rely on a later autosave.

## Simulation boundary

The v12 clock record binds tick, calendar epoch and UTC to the last published excavation transaction. Elapsed simulation milliseconds equal 50 times the tick. Restore checks the journal revision and the saved impact entity projections before exposing the world. A checkpoint set from different boundaries fails to open. Preserve the matching snapshot and both journals together. A v11 snapshot or a directory containing previous-generation journals fails the read-only compatibility preflight.
