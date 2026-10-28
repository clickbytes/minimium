# Prototype operation and verification

This page gives procedures for the current prototype. They are not test results. The 500 connected player / Pentium-class 500 MHz target and the runtime scenarios below remain unverified.

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

Game protocol v8 currently runs over prototype TCP. The server is authoritative for movement, collision, hitscan damage, chat validation, nearby-player snapshots, and local player persistence. A client profile ID is a durable local identifier, not an account credential. Public game traffic is not authenticated or encrypted. The local admin endpoint is loopback-only and separately authenticated.

World snapshot format v3 stores map fingerprint and durable local profile ID, name, position, and health. The server autosaves every 300 seconds by default; `--autosave-seconds 0` disables periodic saves, while controlled shutdown still requests a final snapshot. These behaviors do not establish crash-proof saving or recovery in every failure mode. Protocol v8 and snapshot v3 remain the scope of this implementation; accounts, TLS, inventory, NPCs, zones, an editor, audio, browser clients, and production hosting remain future work.

## Deferred verification plan

No runtime or load scenarios are confirmed by this document. After an explicit final-verification instruction, cover:

- Two independent profiles on one server and rejection of a duplicate active profile.
- The ground-to-stairs-to-platform-to-bridge path, travel beneath the bridge, ramps, and triangle collision.
- Owned-child and attached-server shutdown, plus window/renderer errors without orphaned children.
- Slow admin clients, oversized inputs/outputs, token rejection, and partial I/O.
- Two server launches targeting one data directory.
- Save, restart, recovery, final-save failure, and shutdown error handling.
- Full snapshots, deltas, and player removals.
- Equivalent authoritative outcomes with server lanes 1, 2, and `auto`, using at least 32 active players.
- The 18 prepared load runs: `clustered` and `dispersed` movement, each at 2, 100, and 500 clients, with server lanes 1, 2, and `auto`, for 60 seconds per run. Keep generator worker settings separate from server lane settings and record hardware/configuration.

The scenario and report format are described in [report v2](scenarios/report-v2.md). The 18 prepared 60-second workloads have now been run on one Windows 10 IoT Enterprise LTSC x86_64 host; the outcomes and machine are recorded in the [Epic 002 verification ledger](plan/epic-002-verification.md). A raw client also traversed the demo-map stairs, upper deck, bridge and far platform, climbed a walkable triangle ramp, and stopped at a vertical triangle. A forced snapshot write failure preserved the prior v3 file across a process restart. A 64-client slow-reader run caused bounded snapshot drops; it did not establish whether the 64 KiB byte limit is the rejecting guard. These runs do not establish the Pentium-class 500 MHz target; visual behavior and exact network lane equivalence remain unverified.
