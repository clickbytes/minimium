# Minimium

Minimium is a resource-conscious Rust engine for networked first-person games in full 3D. Local play uses separate client and server processes and the same protocol as remote play. See `docs/architecture.md` for the current design.

## Current state

The workspace is an early playable vertical slice: a desktop 3D client, a separate authoritative server process, basic server-side movement and box collision, nearby-player snapshots, a headless admin CLI, configurable execution lanes, and a simple load generator. Maps use a validated, versioned text format shared by the server and client; the current format supports axis-aligned boxes. Local desktop play starts a server child automatically. Press F1 in the window to open the command console; `--console` starts the separate text client prototype.

This is not yet a production engine or an MMO server. The default sample map is bundled and the client/server also accept `--map PATH`; maps support solid or visual boxes plus visual-only flat-color triangles. Mesh collision, map tools, and streaming remain future work. Game traffic uses a versioned binary protocol over prototype TCP. `world save` stores player records with a map fingerprint and stable local client profile ID; saved characters return to the live world when that profile reconnects. The profile ID is not account authentication, and a crash or power loss can lose changes since the last save. Public-game authentication and encryption, durable inventory and world state, asset cooking, advanced physics, audio, and multi-zone hosting are future work. The Pentium 500 MHz / 500-player target is a performance goal, not a measured result.

## Build

Requires Rust and Cargo.

~~~text
cargo build --workspace
~~~

Open the desktop client. It starts a separate local server process and connects through the game protocol:

~~~text
cargo run -p minimium-client
~~~

To use a map file other than the bundled sample, pass the same map to the local client and server:

~~~text
cargo run -p minimium-client -- --map games/sample/maps/demo.map
~~~

The server defaults to loopback. To run it separately, use two terminals. Start the server in terminal 1 and pass its printed game address to the client in terminal 2:

~~~text
# Terminal 1
cargo run -p minimium-server -- run --threads 1 --data-dir .minimium --map games/sample/maps/demo.map --headless
# Terminal 2
cargo run -p minimium-client -- --connect 127.0.0.1:PORT --map games/sample/maps/demo.map
~~~

`--headless` keeps the server in the foreground for a service manager and disables stdin commands. Use `minimiumctl` from another terminal to manage it. Without `--headless`, the server also accepts commands from its terminal.

Run the renderer without networking with `cargo run -p minimium-client -- --demo`. Use WASD to move and Space to jump. Add `--map PATH` to render a custom map; the server also accepts `--map PATH` and parses it before binding its ports. Remote clients must use the same map as their server. See [docs/maps.md](docs/maps.md) for the format. The in-window F1 console supports `help`, `connect HOST:PORT`, `disconnect`, `fps show`, `net stats`, `ping`, `say NAME`, `state`, `clear`, and `quit`; arrow keys browse history and Tab suggests commands. Run the separate text client with `cargo run -p minimium-client -- --console` for `host local` and direct protocol commands.

The server binds game traffic to `127.0.0.1` by default. Use `--bind 0.0.0.0` only for LAN experiments. The prototype game protocol does not yet authenticate or encrypt public game traffic; production internet hosting is not ready.

Admin commands:

~~~text
cargo run -p minimiumctl -- --data-dir .minimium status
cargo run -p minimiumctl -- --data-dir .minimium players list
cargo run -p minimiumctl -- --data-dir .minimium world save
cargo run -p minimiumctl -- --data-dir .minimium stop
~~~

`world save` queues a snapshot on the server's storage worker. Use `status` to check whether it is pending and whether the last save succeeded.
`stop` waits for the server's final snapshot and reports whether shutdown completed cleanly.

`--threads 1`, `--threads 2`, and `--threads auto` configure execution lanes. One lane runs snapshot work inline; extra lanes use persistent workers and process client snapshots in batches.

Build the load generator with `cargo build -p minimium-loadtest`, then target the port printed by the server:

~~~text
cargo run -p minimium-loadtest -- --address 127.0.0.1:PORT --clients 500 --seconds 20
~~~

The load generator creates one operating-system thread per simulated client. Use it for small repeatable experiments; it is not itself a scalable bot framework.
