# minimium-storage

## Purpose / Non-goals

Owns snapshot serialization and validation, player persistence records and the bounded persistence worker. It does not own live runtime worlds or network transport.

## Public API

`PlayerStore` loads and updates persistent player state. The server captures immutable `WorldSaveInput` values before handing them to `PersistenceWorker`; the worker writes `SaveSnapshot` values.

## Dependencies

Consumes core identity, world and simulation values, environment clock state, telemetry and platform file operations. The player-name byte limit belongs to core identity; storage does not depend on network protocol.

## Data flow

The owning server lane builds a snapshot input from world state. Storage validates and serializes the immutable snapshot, then reports completion through its bounded worker queue.

## Ownership and execution

The server owns `PlayerStore` and captures all live world data on its owner lane. The worker owns only queued immutable snapshots and the configured data directory.

## Invariants and errors

Preserve save format v9, existing legacy migration rules, validation limits, atomic replacement and explicit queue-full errors.

## Commands

No commands. The server runtime owns `world save` dispatch.

## Change guide

Change snapshot format and validation here. Build runtime `WorldSaveInput` values in the server before crossing the worker boundary.

## Verification

Run `rtk cargo check -p minimium-storage -p minimium-server` and architecture/file-length checks. Runtime acceptance is deferred.
