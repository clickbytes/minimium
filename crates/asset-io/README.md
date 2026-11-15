# minimium-asset-io

## Purpose / Non-goals

Owns bounded file access for authored maps, referenced OBJ meshes and world definitions, plus opened prepared GEBCO geography. It does not parse authored formats or generate worlds.

## Public API

`materials::MaterialPackage` verifies file sizes, hashes, KTX2 dimensions and GPU
byte budgets. `ContentCache` reserves capacity across processes, installs only
complete verified digest blobs, and keeps active package blobs under a shared
lease. Its default capacity is 8 GiB and callers may set a different limit.
Each install reserves the unique missing blob bytes and any new receipt before
writing, while protecting all required digests and its receipt from eviction.
Eviction only removes cache-owned digest blobs whose lease can be acquired;
unavailable capacity returns a limit error without exceeding the configured
quota.
Abandoned staging cleanup recognizes only installer-owned digest/receipt names
with run and task identifiers; unrelated `.part` files are left intact.
`material_http::fetch` reads bounded HTTP responses from an explicit endpoint.
`ContentServer` serves only a validated published package through four workers
and a 32-connection bounded queue; request headers and transfer deadlines are
bounded, and shutdown reports workers or snapshot resources that did not finish.
Before binding the listener, it reads one bounded manifest and validates each
variant's exact byte buffers for length, digest, KTX2 format and GPU budgets.
Those same buffers are copied into exclusively created new snapshot files;
workers never reopen source files or use hardlinks. Replacing source files,
including in-place writes of the same length, cannot change a published package.
Only one variant is retained in memory at a time; scratch disk is bounded by
both manifest-declared variants plus the manifest (at most 1 GiB + 64 KiB).
The runtime supplies worker ownership. Prepared Earth geography optionally maps
`worldcover.bin` with `worldcover.sha256`; absence uses procedural coverage without
changing the GEBCO identity or geometry. `examples/validate_materials.rs` is an
offline static package validator; it never starts a game or requests a GPU.

`load_map` opens a map and supplies bounded mesh text to assets. `load_world_definition` reads a bounded source file and delegates validation to assets. `open_earth_package` validates and returns an immutable, shareable package that implements procgen's `PreparedGeography` port; `open_earth_package_cancellable` hashes mapped tiles in 8 MiB chunks and checks the caller's cancellation predicate between chunks. Geography, material package, bounded material read, installation and HTTP fetch failures use `AssetIoError`, with stable categories/codes for missing files, invalid format, unsupported versions, digest mismatches, limits, cancellation, timeout, protocol and I/O; adapter I/O and parser causes remain available through `Error::source`.

## Dependencies

Depends on `minimium-assets` parsers, `minimium-world` map types and the narrow `minimium-procgen::PreparedGeography` interface. It does not depend on application runtimes.

## Data flow

Paths and file bytes enter the adapter; validated `WorldMap` or `WorldDefinition` values leave it. Mesh paths are relative to the map directory and canonicalized before opening.

## Ownership and execution

The calling application selects paths and owns loaded definitions. Reads are bounded by parser limits; long-lived callers can use the cancellable geography opener in a worker. ContentCache owns download reservation, cross-process coordination, package leases and eviction; clients own its configured root and lifetime.

The HTTP server's `Drop` requests stop without joining. Explicit
`shutdown(timeout)` owns the bounded join. Server and workers share ownership of
the private snapshot; failed staging removes its created files, and normal
shutdown removes them after workers exit. A worker that outlives the deadline
keeps the snapshot until its final reference drops. Cleanup removes only owned
files and the now-empty owned directory, never recursively traversing a source
tree. Cleanup failures emit `asset_io.material_snapshot_cleanup_failed`; explicit
shutdown also returns `material-http-snapshot` when cleanup fails. Dropping after
the deadline does not start another wait.

## Invariants and errors

The adapter enforces map-directory containment for mesh files and caps reads at format limits. File errors retain `io::Error`; parse errors become `InvalidData`.

## Commands

None.

## Change guide

Add file or package access here. Keep format validation in assets and generation in procgen.

## Verification

Run `rtk cargo check -p minimium-asset-io -p minimium-client -p minimium-server` and architecture/file-length checks. Runtime acceptance is deferred.
