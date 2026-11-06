# minimium-storage

## Purpose / Non-goals

Owns snapshot serialization and validation, player persistence records and the bounded persistence worker. It does not own live runtime worlds or network transport.

## Public API

`PlayerStore` loads and updates persistent player state. The server captures immutable `WorldSaveInput` values before handing them to `PersistenceWorker`; the worker writes `SaveSnapshot` values.

## Dependencies

Consumes core identity, world and simulation values, environment clock state, telemetry and platform file operations. The player-name byte limit belongs to core identity; storage does not depend on network protocol.

## Data flow

The owning server lane builds a snapshot input from world state. `PlayerStore` updates its profile shards and captures immutable `SaveSnapshot` values; the persistence worker queues and writes those snapshots. The format adapter parses the header, then validates clock, world, sector and player records in order, or serializes the same records to a temporary file before atomic replacement.

## Ownership and execution

The server owns `PlayerStore` and captures all live world data on its owner lane. Profile operations and legacy location migration are separate `player_store` modules; snapshot capture keeps the public `PlayerStore::save_snapshot` entry point. The worker owns its bounded request channel, save-status bookkeeping, queued immutable snapshots and configured data directory. Its shutdown path sends a final snapshot, waits for the reply, joins the worker and collects pending completion status.

## Invariants and errors

Preserve save format v9, existing legacy migration rules, validation limits, atomic replacement and explicit queue-full errors.

## Commands

No commands. The server runtime owns `world save` dispatch.

## Change guide

Change snapshot format and validation here. Build runtime `WorldSaveInput` values in the server before crossing the worker boundary.

## Verification

Run `rtk cargo check -p minimium-storage -p minimium-server` and architecture/file-length checks. Runtime acceptance is deferred.

Snapshot v9 retains the unchanged records for generation revisions 1..4.
A present v5 planet definition appends four u32 decimal fields after its package
digest: peak start metres, full metres, multiplier permille and exponent.
Parsing is gated by the manifest's generator revision, including embedded
manifests. Old manifests receive the neutral curve; invalid settings are rejected.
This is a generator-specific record extension, not a migration of existing worlds.
