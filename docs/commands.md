# Command and option reference

This page documents the current parsers and dispatchers. For operational sequences, see [prototype procedures](prototype.md); for metric meaning and limitations, see [performance policy](performance.md).

## Server

~~~text
minimium-server run [--threads 1|2|auto] [--max-players N] [--bind IP] [--game-port PORT] [--data-dir PATH] [--map PATH] [--autosave-seconds N] [--headless]
minimium-server --version
~~~

The defaults are one execution lane, 512 active players, loopback bind (`127.0.0.1`), an OS-selected game port (`0`), data directory `.minimium`, bundled map, and autosave every 300 seconds. `--threads` accepts 1 through 64 lanes or `auto` (host parallelism, subject to the 64-lane limit). `--max-players` accepts 1 through 512. `--autosave-seconds` accepts 0 through 604800; zero disables periodic saves. `--headless` leaves the server in the foreground but disables terminal commands. Without it, terminal commands use the same command registry as the admin CLI. Game traffic binds to the selected IP; the administrative listener is loopback-only.

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

`--profile-dir PATH` selects this client's durable local player profile. `--server-data-dir PATH` selects the shared local server's storage and discovery directory. They are independent: use separate profile directories for separate local players and the same server data directory when they should share one server. If omitted, the default is `.minimium-local`; with `--map`, it is `.minimium-local/map-<map-fingerprint>`.

The bundled map is used unless `--map PATH` is supplied. A custom map must be provided to every client and the server. The welcome fingerprint detects an accidental map mismatch; it is not authentication.

### Text-client commands

The text client accepts `host local [--threads 1|2|auto]`, `connect HOST:PORT`, `help`, `ping`, `move X Z`, `jump`, `fire`, `say TEXT`, `name NAME`, `state`, `disconnect`, `fps show`, and `quit`/`exit`. `host local` starts or attaches to the local server configured by startup paths. It stores an owned server guard before later connection setup; only an owned child is stopped on exit. An attached server is left running.

### In-window console commands

Press F1 to open or close the console. It accepts `help`, `connect HOST:PORT`, `disconnect`, `fps show`, `net stats`, `ping`, `say TEXT`, `name NAME`, `state`, `clear`, and `quit`. Arrow keys browse history and Tab suggests commands. Lock the mouse and left-click to fire. `fps show` reports frame interval and render-loop work summaries; `net stats` reports application bytes, latest round-trip time, and nearby-player count.

## Load generator

~~~text
minimium-loadtest --address HOST:PORT [--scenario heartbeat|movement|combat|chat|clustered|dispersed] [--clients N] [--seconds N] [--threads 1|2|auto] [--format text|json] [--scenario-id TEXT] [--cpu-label TEXT] [--cpu-mhz N] [--server-data-dir PATH]
~~~

`--address` is required. Client count is 1 through 10000; duration is 1 through 86400 seconds. Generator `--threads` chooses 1, 2, or up to 2 workers for `auto`; this is separate from server `--threads`. `--format` is `text` or `json`. The optional CPU fields are user-supplied labels and clock values, not machine probes. `--server-data-dir` requests authenticated server status and metrics samples before and after the run; capture failures are reported as unavailable samples. It does not sample the server on every tick.

- `heartbeat` sends a heartbeat every second.
- `movement` sends movement input at 20 Hz and uses heartbeat traffic for RTT sampling.
- `combat` adds a shot every 300 ms to movement.
- `chat` staggers chat traffic at 25-second intervals.
- `clustered` and `dispersed` use deterministic movement directions by simulated-client index; they are intended for the final spatial-distribution scenarios.

The JSON output is schema version 2. Field definitions and the prepared scenario manifest belong in [report-v2.md](scenarios/report-v2.md). Reports contain application protocol bytes, not TCP/IP framing or retransmissions.
