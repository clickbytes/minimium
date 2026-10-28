# Minimium

The current command-line options, defaults, command aliases and per-program thread limits are documented in [the command reference](docs/commands.md).

Minimium is a resource-conscious Rust engine for networked first-person games in full 3D. Local play uses separate client and server processes and the same protocol as remote play. See [the vision](docs/vision.md), [architecture](docs/architecture.md), and [prototype operating procedures](docs/prototype.md).

## Current state

The workspace contains a desktop 3D client, a separate authoritative server, server-side movement and collision, hitscan combat, global text chat, nearby-player snapshots, a headless admin CLI, configurable execution lanes, and a load generator. The client can start a local server process or attach to a separately run server. DNS lookup, remote connection attempts, and local server startup run on a connector worker so the desktop event loop can continue; after connection, a shared nonblocking TCP session is polled by the desktop or text-client loop. The client HUD shows health and confirmed hits; press F1 for the in-window console, lock the mouse and left-click to fire, and use `say TEXT` (or `chat TEXT`) to chat. `--console` selects the separate text-client prototype.

Maps use a validated, versioned text format shared by the client and server. The current format supports axis-aligned boxes, flat-color triangles, and static Wavefront OBJ meshes. Imported mesh faces are expanded into the triangle rendering and collision pipeline at startup. Textures, cooked/batched assets, streaming, and map-authoring tools remain future work. See [map format and mesh examples](docs/maps.md).

Game traffic uses binary protocol v8 over prototype TCP. A client sends its 16-byte local profile ID first; this lets the server restore that profile's character but is not account authentication. World snapshot format v3 stores the profile ID, name, position, health, and static-map fingerprint. Controlled shutdown requests a final save; the server autosaves every 300 seconds by default (`--autosave-seconds 0` disables periodic saves). Manual saves queue an immutable snapshot for a storage worker and complete asynchronously; a crash or power loss can still lose changes since the last completed save. Public-game authentication and encryption, durable inventory and full world state, advanced physics, audio, browser clients, and multi-zone hosting remain future work. The 500 connected player / Pentium-class 500 MHz target is a research target, not a measured result; no gameplay or load results are claimed here.

## Experimental planet and dungeon

A new procedural [cube-sphere + dungeon walkthrough](docs/worlds.md) is available as a separate map; the existing default demo is unchanged.

```sh
cargo build --workspace
cargo run -p minimium-client -- --map games/sample/maps/planet_dungeon.map
```

Start on the planet, walk toward the orange portal, explore the flat-gravity dungeon and use the blue exit to return. This is one shared-world prototype, not yet instanced or streaming.

## Build and run

Requires Rust and Cargo.

~~~text
cargo build --workspace
~~~

Run the desktop client with its default local server behavior:

~~~text
cargo run -p minimium-client
~~~

After updating the source, build the whole workspace before launching the
client. `cargo run -p minimium-client` builds the client package only; its
separately launched server executable can otherwise be missing or outdated.

For a custom map, pass the same path to client and server. For example, this starts a local server automatically:

~~~text
cargo run -p minimium-client -- --map games/sample/maps/demo.map
~~~

Run without networking with `--demo`. Use WASD to move and Space to jump. The in-window console supports `help`, `connect HOST:PORT`, `disconnect`, `fps show`, `net stats`, `ping`, `say TEXT`, `name NAME`, `state`, `clear`, and `quit`. Arrow keys browse history; Tab suggests commands. To use the text client, add `--console`; it supports `host local`, `connect HOST:PORT`, `move`, `jump`, `fire`, `say`, `name`, `state`, and `quit`. See [commands](docs/commands.md) for all flags and commands.

## One server with two independent local profiles

Use a new `.minimium-epic001` directory for the current map fingerprint. Keep any existing `.minimium` or `.minimium-local` directories; do not delete or reset old saves. Start the first client in terminal 1:

~~~text
cargo run -p minimium-client -- --map games/sample/maps/demo.map --server-data-dir .minimium-epic001/server --profile-dir .minimium-epic001/profile-a --threads 1
~~~

Start the second client in terminal 2 with the same server data directory and a distinct profile directory:

~~~text
cargo run -p minimium-client -- --map games/sample/maps/demo.map --server-data-dir .minimium-epic001/server --profile-dir .minimium-epic001/profile-b --threads 1
~~~

The first client starts and owns the local server child. The second authenticates the local endpoint and attaches; it never stops an attached server. Keep the owner open while the second client is connected. On owner exit, graceful shutdown can take up to 120 seconds; fallback kill and wait apply only to the owned child. The independent profile directories give the clients distinct persistent player identities.

## Standalone server and admin CLI

The server defaults to loopback and an OS-selected game port. To run it separately on a fixed port, use terminal 1:

~~~text
cargo run -p minimium-server -- run --threads 1 --game-port 27015 --data-dir .minimium-epic001/server --map games/sample/maps/demo.map --headless
~~~

Clients may auto-discover this local server by using the same `--server-data-dir` and omitting `--connect`, or explicitly connect to `127.0.0.1:27015`. Explicit `--connect` does not launch or own a local server. `--headless` keeps the server in the foreground and disables stdin commands; it does not detach the process. `--bind 0.0.0.0` accepts remote game connections, but the prototype game protocol has no public-game authentication or encryption, so public internet hosting is not ready.

Run admin commands from another terminal:

~~~text
cargo run -p minimiumctl -- --data-dir .minimium-epic001/server status
cargo run -p minimiumctl -- --data-dir .minimium-epic001/server players list
cargo run -p minimiumctl -- --data-dir .minimium-epic001/server metrics show
cargo run -p minimiumctl -- --data-dir .minimium-epic001/server world save
cargo run -p minimiumctl -- --data-dir .minimium-epic001/server stop
~~~

`world save` queues a snapshot; check `status` for completion and final result. `stop` waits for the final snapshot and server termination and returns an error if saving or shutdown failed. See [the command reference](docs/commands.md).

## Load generator and verification

Build and run the load generator against the server's game address:

~~~text
cargo build -p minimium-loadtest
cargo run -p minimium-loadtest -- --address 127.0.0.1:27015 --clients 100 --seconds 20 --scenario movement --format json
~~~

Supported scenarios are `heartbeat`, `movement`, `combat`, `chat`, `clustered`, and `dispersed`. Generator worker threads and server execution lanes are separate settings. Schema v2 can include authenticated before/after server samples when passed `--server-data-dir PATH`; report definitions and planned scenarios are documented in [performance policy](docs/performance.md) and [report v2](docs/scenarios/report-v2.md).

The prepared 18-run clustered/dispersed matrix has not been run. Runtime verification, including lane-equivalent outcomes at 1/2/`auto` with at least 32 players, remains deferred until an explicit final-verification instruction. Nothing here establishes gameplay correctness, measured capacity, or performance on a 500 MHz machine.
