# Command and option reference

This page documents the current parsers and dispatchers. For operational sequences, see [prototype procedures](prototype.md); for metric meaning and limitations, see [performance policy](performance.md).

## Server

~~~text
minimium-server run [--threads 1..64|auto] [--max-players N] [--bind IP] [--game-port PORT] [--data-dir PATH] [--map PATH] [--autosave-seconds N] [--headless]
minimium-server --version
~~~

The defaults are one execution lane, 512 active players, loopback bind (`127.0.0.1`), an OS-selected game port (`0`), data directory `.minimium`, bundled map, and autosave every 300 seconds. `--threads` accepts 1 through 64 lanes or `auto` (host parallelism, subject to the 64-lane limit). `--max-players` accepts 1 through 512. `--autosave-seconds` accepts 0 through 604800; zero disables periodic saves. `--headless` leaves the server in the foreground but disables terminal commands. Without it, terminal commands use the same command registry as the admin CLI. Game traffic binds to the selected IP; the administrative listener is loopback-only.

The shared server command parser is owned by `minimium-platform`; terminal and admin control use the same schema.

Up to eight queued server commands execute at a tick boundary before world inputs and simulation. `world save` acknowledges queuing, not completion of the disk write; `stop` enters controlled shutdown after the current tick.

`--map PATH` loads and validates the map before binding. `--game-port 0` asks the OS for an available port; a fixed port is useful when connecting explicitly. The server writes its selected game/admin endpoint into its data directory. Keep the directory private to one server process; an exclusive OS file lock prevents a second server from owning the same directory.

To include allocation counters in `metrics show`, enable the server feature at build/run time:

~~~text
cargo run -p minimium-server --features allocation-metrics -- run --data-dir .minimium
~~~

## Server commands

The same commands are available through the server terminal (unless `--headless`) and `minimiumctl`:

| Command | Meaning |
| --- | --- |
| `help` | Print the command list. |
| `status` | Report running state, active players, configured lanes, ticks, save state, player cap, map fingerprint, tick/snapshot intervals, autosave interval, protocol version, and save version. |
| `players list` | List connected player IDs, names, and positions, or report no players. |
| `metrics show` | Print runtime, network, queue, collision, memory, process, and optional allocation metrics. Field names are documented in [performance policy](performance.md). |
| `world save` | Queue one snapshot on the persistence worker. A successful reply means queued, not completed; use `status` to inspect `save_pending` and `save_last`. A second save is rejected while one is pending. |
| `stop` | Request controlled shutdown. The server saves a final snapshot, reports the final result, and releases the data-directory lock. |

The snapshot for `world save` captures the committed world at the command's execution point, before that tick applies its pending game inputs. A queued acknowledgement does not mean inputs still waiting for that tick are included. Controlled `stop` applies the pending input batch before taking its final snapshot.

`minimiumctl` syntax:

~~~text
minimiumctl [--data-dir PATH] <help|status|players list|metrics show|world save|stop>
~~~

The data directory defaults to `.minimium`. `minimiumctl stop` waits up to 120 seconds for the captured instance to finish, verifies its private owner marker, checks the legacy `server.shutdown` result, and waits for lock release. It exits with an error if the final save or shutdown failed. The legacy result body remains `<admin-port>\tOK` or `<admin-port>\tERR\t<message>`; the separate private owner sidecar binds that result to the instance. Neither file should be treated as a public credential store.

## Client

~~~text
minimium-client [--connect HOST:PORT] [--map PATH] [--profile-dir PATH] [--server-data-dir PATH] [--threads 1|2|auto] [--demo] [--console]
~~~

Each option may appear once. Unknown options and missing values are errors. `--threads` accepts `1`, `2`, or `auto`; it controls a server started by local play and defaults to `1`. `--connect` connects to an explicit game address and does not start or own a server. Without `--connect` or `--demo`, the desktop client starts a local server if needed and connects through the normal game protocol. `--demo` renders without a game connection. `--console` selects the text client instead of the desktop renderer.

`--profile-dir PATH` selects this client's durable local player profile. `--server-data-dir PATH` selects the shared local server's storage and discovery directory. They are independent: use separate profile directories for separate local players and the same server data directory when they should share one server. If omitted, both resolve to `.minimium-local`; with `--map`, both default to `.minimium-local/map-<map-fingerprint>`. The persisted profile ID is a reconnect identifier, not account authentication.

The bundled map is used unless `--map PATH` is supplied. A custom map must be provided to every client and the server. The welcome fingerprint detects an accidental map mismatch; it is not authentication.

Both frontends use one connection controller. It starts a background connector worker for DNS, sequential remote address attempts, or local server startup. The 10-second attempt timeout bounds when the front end accepts the result; blocking resolver or startup work cannot always be cancelled, so a timed-out/cancelled attempt remains busy until its worker exits. A replaced owned local server is cleaned up on a tracked worker before another attempt is allowed. After the connector returns a socket, the foreground client loop polls a shared nonblocking session; the session itself does not own a thread. The session sends Identify first, requires Welcome with the expected map fingerprint, queues app messages only when connected, and handles heartbeat timers. The initial Identify/Welcome exchange has a five-second client timeout; connected clients send pings every 10 seconds, disconnect after 20 seconds without a pong, and the server drops a client after 30 seconds without a valid game message.

### Text-client commands

The text client and in-window console use the same command catalog and parser. Both accept `help`, `host local [--threads 1|2|auto]`, `connect HOST:PORT`, `disconnect`, `fps show`, `net stats`, `ping`, `move X Z`, `jump`, `fire`, `say TEXT` (alias `chat`), `name NAME`, `state`, `clear`, and `quit` (alias `exit`). `host local` starts or attaches to the local server configured by startup paths. It stores an owned server guard before later connection setup; only an owned child is stopped on exit. An attached server is left running. The terminal frontend renders state/events as text; the desktop frontend provides F1 console history and tab completion from the same catalog.

### In-window console commands

Press F1 to open or close the console. It uses the shared command catalog above. Arrow keys browse history and Tab suggests canonical commands and aliases. Lock the mouse and left-click to fire. `fps show` reports frame interval and render-loop work summaries; `net stats` reports application bytes, latest round-trip time, and nearby-player count.

## Load generator

~~~text
minimium-loadtest --address HOST:PORT [--scenario heartbeat|movement|combat|chat|clustered|dispersed] [--clients N] [--seconds N] [--threads 1|2|auto] [--format text|json] [--scenario-id TEXT] [--cpu-label TEXT] [--cpu-mhz N] [--server-data-dir PATH]
~~~

`--address` is required. Client count is 1 through 10000; duration is 1 through 86400 seconds. Generator `--threads` chooses 1, 2, or up to 2 workers for `auto`; this is separate from server `--threads`. `--format` is `text` or `json`. The optional CPU fields are user-supplied labels and clock values, not machine probes. `--server-data-dir` requests authenticated `status` and `metrics show` samples before setup, at measurement start, and after the run. Accepted samples require the configured literal loopback address to match the server-reported game address and endpoint game port, and require one stable endpoint identity (including token) across captures; capture or identity failure clears all three samples. The private token and detailed admin errors are not serialized. This check associates the sample with a local listener, not a cryptographic identity proof. Sampling is at run boundaries, not every tick.

- `heartbeat` sends a heartbeat every second.
- `movement` sends movement input at 20 Hz and uses heartbeat traffic for RTT sampling.
- `combat` adds a shot every 300 ms to movement.
- `chat` staggers chat traffic at 25-second intervals.
- `clustered` and `dispersed` use deterministic movement directions by simulated-client index; they are intended for the final spatial-distribution scenarios.

The JSON output is schema version 2. Field definitions and the prepared scenario manifest belong in [report-v2.md](scenarios/report-v2.md). Reports contain application protocol bytes, not TCP/IP framing or retransmissions. The load generator is implementation scaffolding; prepared workloads are not runtime results, and no gameplay correctness, capacity, or performance claim follows from a static code review.
