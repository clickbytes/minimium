# Prototype operation and verification

This page gives procedures for the current prototype. Recorded outcomes are in the [Epic 002 verification ledger](plan/epic-002-verification.md). The Pentium-class 500 MHz target remains unverified; use the ledger to distinguish completed runtime scenarios from the procedures below.

## Build and map

Build the workspace with Rust and Cargo:

~~~text
cargo build --workspace
~~~

The bundled demo map is `games/sample/maps/demo.map`. It includes an elevated path made from steps, a platform and bridge, a route beneath the bridge, and separate ramp/triangle examples. A file map may also contain flat-color boxes, triangles, and static OBJ meshes; mesh faces are expanded into triangle geometry at startup. See [map format](maps.md) for parser limits and the mesh example.

Use the same map path at the server and every client. The client compares the canonical static-map fingerprint in the server welcome message. A mismatch disconnects the client.

## One local server with two independent clients

Use a new `.minimium-epic001` storage tree for this map fingerprint. Do not delete, reset, or overwrite older `.minimium` or `.minimium-local` saves to make the example work. Keep the owner client open while the attached client is in use.

In terminal 1, start the first desktop client:

~~~text
cargo run -p minimium-client -- --map games/sample/maps/demo.map --server-data-dir .minimium-epic001/server --profile-dir .minimium-epic001/profile-a --threads 1
~~~

With no server in that directory, this client starts a local server child, owns its lifetime, and connects through the game protocol. In terminal 2, start a second client with the same server directory and a different profile directory:

~~~text
cargo run -p minimium-client -- --map games/sample/maps/demo.map --server-data-dir .minimium-epic001/server --profile-dir .minimium-epic001/profile-b --threads 1
~~~

The second client authenticates the local admin endpoint and verifies the server lock before attaching. It does not stop the server when it exits. The first client owns the child and requests controlled shutdown on exit; graceful stop can take up to 120 seconds, after which the owner may kill and reap only its own child if necessary. Do not close the owner while the attached client still needs the server.

Terminal and desktop clients share the same connection controller and session. Ownership, cancellation, late-result cleanup and timeout behavior remain scenarios for final verification.

Both clients can be text clients by adding `--console`. In that mode, type `host local` in each terminal; the first starts the server and the second attaches. `--threads` applies when a local child is started.

The two `--profile-dir` paths create distinct local player IDs while `--server-data-dir` shares one server and one save store. Omitting explicit profile directories makes clients use the same default profile path, so use distinct paths for independent players.

## Standalone server

Start the server in one terminal with a fixed game port and a dedicated data directory:

~~~text
cargo run -p minimium-server -- run --threads 1 --max-players 512 --game-port 27015 --data-dir .minimium-epic001/server --map games/sample/maps/demo.map --headless
~~~

To attach through local endpoint discovery, launch clients with the same `--server-data-dir`, distinct `--profile-dir` values, and the same `--map`, omitting `--connect`. To connect explicitly, use `--connect 127.0.0.1:27015` with a distinct profile directory on each client; explicit connect does not use or own a local server. The separate server remains in the foreground. `--headless` disables its stdin command reader; it does not background the process.

The OS lock rejects another server process that tries to own this same data directory. Manage the standalone server from another terminal:

~~~text
cargo run -p minimiumctl -- --data-dir .minimium-epic001/server status
cargo run -p minimiumctl -- --data-dir .minimium-epic001/server players list
cargo run -p minimiumctl -- --data-dir .minimium-epic001/server metrics show
cargo run -p minimiumctl -- --data-dir .minimium-epic001/server world save
cargo run -p minimiumctl -- --data-dir .minimium-epic001/server stop
~~~

`world save` acknowledges that a snapshot was queued. Poll `status` until `save_pending=0` and inspect `save_last`. `stop` waits for the final save result, the matching private shutdown-owner marker, and lock release. A failed final save or shutdown returns an error; do not interpret a `stopping` acknowledgement as completion.

## Current behavior and limits

Game protocol v12 currently runs over prototype TCP. The server is authoritative for movement, collision, hitscan damage, chat validation, nearby-player snapshots, world membership and portal transfers. A client profile ID is a durable local identifier, not an account credential. Public game traffic is not authenticated or encrypted. The local admin endpoint is loopback-only and separately authenticated.

World snapshot format v6 stores every loaded world manifest and map fingerprint, the durable player profile, instance, quantized position/orientation, health and return location, and revisioned logical object-state sets per sector. It reads v2 through v5; v2/v3 data remains on a loaded legacy map selected by fingerprint. The server autosaves every 300 seconds by default; `--autosave-seconds 0` disables periodic saves, while controlled shutdown still requests a final snapshot. These behaviors do not establish crash-proof recovery. The demo has no object mutation interaction; geometry edits, accounts, TLS, inventory, NPCs, zones, audio, browser clients, and production hosting remain future work.

## Runtime verification coverage

The final runtime phase ran on 2026-10-28. The [verification ledger](plan/epic-002-verification.md) records commands, evidence paths, results, and remaining limits. It covers two independent profiles and duplicate rejection, the demo-map route and triangle collision, Owned/Attached lifecycle, data-directory locking, save/restart/recovery and v2 migration, admin request bounds and a delayed 21,398-byte admin response with a concurrent `status`, 32-player lane equivalence, and all 18 prepared load runs. The controlled slow-reader run confirmed bounded snapshot drops but did not identify whether the 64 KiB byte cap or 256-frame cap rejected them; deterministic queue tests cover the two guards independently. A 512-client single-reader pressure probe did not fill the application queue on Windows loopback.

Still outstanding are desktop visual inspection, reaching the byte cap with real network traffic, and verification on Pentium-class 500 MHz hardware. The server flush unit test exercises `WriteZero` and partial-write handling through a deterministic writer; it does not inject those outcomes from an OS TCP socket. No gameplay or capacity claim should be inferred for the unverified cases.

The scenario and report format are described in [report v2](scenarios/report-v2.md). The 18 prepared 60-second workloads have now been run on one Windows 10 IoT Enterprise LTSC x86_64 host; the outcomes and machine are recorded in the [Epic 002 verification ledger](plan/epic-002-verification.md). A raw client also traversed the demo-map stairs, upper deck, bridge and far platform, climbed a walkable triangle ramp, and stopped at a vertical triangle. A forced snapshot write failure preserved the prior v3 file across a process restart. A 64-client slow-reader run caused bounded snapshot drops; it did not establish whether the 64 KiB byte limit is the rejecting guard. A controlled network run confirmed bitwise lane equivalence for 32 clients and 100 movement updates. These runs do not establish the Pentium-class 500 MHz target; visual behavior remains unverified.
