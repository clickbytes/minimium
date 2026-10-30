# Command and option reference

This page documents the current parsers and dispatchers. For operational sequences, see [prototype procedures](prototype.md); for metric meaning and limitations, see [performance policy](performance.md).

| Program | Options and commands | Defaults and parsing |
| --- | --- | --- |
| Server | `run`, `--seed`, `--planet-config`, `--environment-config`, `--threads`, `--max-players`, `--bind`, `--game-port`, `--data-dir`, `--map`, `--world-definition`, `--autosave-seconds`, `--headless`, `--help`/`-h`, `--version` | One lane, 512 players, loopback, port 0, `.minimium`, seeded generated worlds, 300 s autosave. `--environment-config` initializes a new world's clock only; it is rejected when world data already exists. |
| Control | `--data-dir`, `--help`/`-h`, server commands below | `.minimium`; help is handled before command parsing. Only the first `--data-dir` is consumed. |
| Client | `--connect`, `--map`, `--world-definition`, `--profile-dir`, `--server-data-dir`, `--threads`, `--sky-quality`, `--demo`, `--console`, `--require-earth`, `--help`/`-h` | One local-server lane, standard sky quality, seeded demo worlds and `.minimium-local` paths; repeated options are errors. Demo conflicts with connect and console. `--require-earth` requires the desktop client and conflicts with demo. `--map` and `--world-definition` are mutually exclusive. |
| Load generator | `--address`, `--scenario`, `--clients`, `--seconds`, `--threads`, `--format`, `--scenario-id`, `--cpu-label`, `--cpu-mhz`, `--server-data-dir`, `--help`/`-h` | Address required; 100 clients, 10 s, heartbeat, auto workers and text output. Repeated option names are errors. |

Lane types remain application-owned: the server accepts `1..=64` or `auto`, the client accepts `1`, `2` or `auto` for a locally started server, and the generator accepts `1`, `2` or `auto` with at most two workers. Their different meanings and limits do not justify a shared policy type. The server's existing invalid-value message mentions `1, 2, or auto` even though values through 64 are accepted; the range check and its separate error remain unchanged.

## Server

~~~text
minimium-server run [--seed DECIMAL|0xHEX] [--planet-config PATH] [--environment-config PATH] [--threads 1..64|auto] [--max-players N] [--bind IP] [--game-port PORT] [--data-dir PATH] [--map PATH | --world-definition PATH] [--autosave-seconds N] [--headless]
minimium-server --version
~~~

The defaults are one execution lane, 512 active players, loopback bind (`127.0.0.1`), an OS-selected game port (`0`), data directory `.minimium`, seeded planet/dungeon demo worlds, and autosave every 300 seconds. `--threads` accepts 1 through 64 lanes or `auto`; `--max-players` accepts 1 through 512. `--autosave-seconds` accepts 0 through 604800; zero disables periodic saves. `--headless` leaves the server in the foreground but disables terminal commands. Without it, terminal commands use the same command registry as the admin CLI. Game traffic binds to the selected IP; the administrative listener is loopback-only.

`--environment-config PATH` reads at most 16 KiB of UTF-8 RON schema 1 with unknown fields rejected. It initializes `mode`, optional `initial_utc`, `scale_ppm`, and `offline_policy` for a new world. `real_time` requires no initial date and exactly 1x; `time_scale` uses the configured date or the current UTC. Examples are [real-time.ron](../presets/environment/real-time.ron) and [time-scale.ron](../presets/environment/time-scale.ron). The option is rejected if `world.seed`, `world.planet.ron`, or `world.snapshot` already exists. Existing v8/v9 snapshots restore their saved clock; v2–v7 snapshots initialize RealTime from current UTC and retain that value in the next completed save.

`--seed` accepts a decimal or `0x` hexadecimal 128-bit value for a new generated
world. If omitted, the server draws one random seed. It records the seed in
`world.seed`; later starts use that seed and reject a different supplied value.
New procedural planets have a deterministic radius between 617.5 and 682.5 km.
`start-procedural.ps1` supplies the procedural RON preset. Both scripts accept
`-DataDir` and optional `-Seed`. `start-earth.ps1` builds and opens the desktop
client; the client starts or attaches to the server in that data directory and
owns a server it starts. It passes `--require-earth`, so the desktop keeps the
demo scene hidden until an Earth manifest is accepted and keeps connection
errors visible in the window title. `-ProfileDir` selects a separate client profile
directory (by default, `${DataDir}-profile`). For the smaller Earth package, run
`import-gebco-lite.ps1`. It streams the 1-arcminute overview of the
[GEBCO 2026 ice-surface grid](https://www.gebco.net/data-products/gridded-bathymetry-data#global)
from the Australian Antarctic Division's COG mirror into
`third_party/gebco-2026` (about 467 MB of raw heights, ignored by Git).
It requires `uv`; the wrapper supplies Rasterio in a temporary environment.
The 1-arcminute coast and relief are coarser than GEBCO's native 15-arcsecond
grid. `start-earth.ps1` defaults to this package and accepts `-PackageDir`.
World saves remain under `%LOCALAPPDATA%/Minimium` by default.

For full resolution, download the GEBCO ice-surface netCDF archive, extract
`GEBCO_2026.nc`, install GDAL's `gdal_translate`, then run
`import-gebco.ps1 -GridPath GEBCO_2026.nc`. It writes eight checked tiles to
`third_party/gebco-2026` by default. A client started from the repository root
uses that package automatically; a client elsewhere sets
`MINIMIUM_EARTH_PACKAGE` to its local copy of the selected package.

The shared server command parser is owned by `minimium-platform`; terminal and admin control use the same schema.

Up to eight queued server commands execute at a tick boundary before world inputs and simulation. `world save` acknowledges queuing, not completion of the disk write; `stop` enters controlled shutdown after the current tick.

`--map PATH` loads a static map; `--world-definition PATH` loads a versioned manifest and authored dungeon blueprint. They cannot be combined. The server validates either file before binding. See [world contracts](worlds.md) for the authored definition format. `--game-port 0` asks the OS for an available port; a fixed port is useful when connecting explicitly. The server writes its selected game/admin endpoint into its data directory. Keep the directory private to one server process; an exclusive OS file lock prevents a second server from owning the same directory.

To include allocation counters in `metrics show`, enable the server feature at build/run time:

~~~text
cargo run -p minimium-server --features allocation-metrics -- run --data-dir .minimium
~~~

## Server commands

The same commands are available through the server terminal (unless `--headless`) and `minimiumctl`:

| Command | Meaning |
| --- | --- |
| `help` | Print the command list. |
| `status` | Report running state, active players, configured lanes, ticks, accepted/rejected clock probes, environment-sync messages, save state, player cap, map fingerprint, tick/snapshot intervals, autosave interval, protocol version, and save version. |
| `players list` | List connected player IDs, names, and positions, or report no players. |
| `metrics show` | Print runtime, network, queue, collision, memory, process, and optional allocation metrics. Field names are documented in [performance policy](performance.md). |
| `world save` | Capture players, sectors and the clock together, then queue one v9 snapshot on the persistence worker. A successful reply means queued, not completed; use `status` to inspect `save_pending` and `save_last`. A second save is rejected while one is pending. |
| `stop` | Request controlled shutdown. The server saves a final snapshot, reports the final result, and releases the data-directory lock. |
| `time status` | Show clock ID, revision, session nonce, mode, UTC milliseconds, rate in ppm and decimal form, offline policy, fault, last UTC correction and pending sync revision. |
| `time mode real_time` | Anchor to current server UTC at 1x. |
| `time mode time_scale` | Continue from the current world date, preserving its TimeScale rate or using 1x when switching from RealTime. |
| `time set RFC3339_UTC` | Set a UTC date and switch to TimeScale; preserve an existing TimeScale rate or use 1x. |
| `time scale DECIMAL` | Set a nonnegative 0.1–1,000,000x rate and switch to TimeScale; zero pauses only the environment clock. |
| `time offline pause` / `time offline advance` | Set the restart policy persisted in save v9. `pause` resumes the saved world date; `advance` adds nonnegative wall-clock downtime multiplied by the saved scale. |

Rates accept decimal digits and an optional dot with one to six fractional digits, parsed exactly as ppm. Dates accept RFC3339 UTC with `Z` or `+00:00`, zero to three millisecond digits, and no leap second. Every accepted mutation increments the clock revision; invalid input is rejected without applying a change. Physical simulation, cooldowns, heartbeat and deadlines continue in real time.

The snapshot for `world save` captures the committed world at the command's execution point, before that tick applies its pending game inputs. A queued acknowledgement does not mean inputs still waiting for that tick are included. Controlled `stop` applies the pending input batch before taking its final snapshot.

`minimiumctl` syntax:

~~~text
minimiumctl [--data-dir PATH] <help|status|players list|metrics show|world save|stop|time ...>
~~~

The data directory defaults to `.minimium`. `minimiumctl stop` waits up to 120 seconds for the captured instance to finish, verifies its private owner marker, checks the legacy `server.shutdown` result, and waits for lock release. It exits with an error if the final save or shutdown failed. The legacy result body remains `<admin-port>\tOK` or `<admin-port>\tERR\t<message>`; the separate private owner sidecar binds that result to the instance. Neither file should be treated as a public credential store.

## Client

~~~text
minimium-client [--connect HOST:PORT] [--map PATH | --world-definition PATH] [--profile-dir PATH] [--server-data-dir PATH] [--threads 1|2|auto] [--sky-quality low|standard] [--demo] [--console]
~~~

Each option may appear once. Unknown options and missing values are errors. `--threads` accepts `1`, `2`, or `auto`; it controls a server started by local play and defaults to `1`. `--sky-quality` accepts `low` or `standard` and defaults to `standard`; `low` disables the local shadow map while keeping the same astronomy and star catalogue. It changes renderer resources without restarting the window, device, server, or connection. The text client accepts this shared option but reports that rendering quality is unavailable there. `--connect` connects to an explicit game address and does not start or own a server. Without `--connect` or `--demo`, the desktop client starts a local server if needed and connects through the normal game protocol. `--demo` renders without a game connection and cannot be combined with `--connect` or `--console`. `--console` selects the text client instead of the desktop renderer.

`--profile-dir PATH` selects this client's durable local player profile. `--server-data-dir PATH` selects the shared local server's storage and discovery directory. They are independent: use separate profile directories for separate local players and the same server data directory when they should share one server. If omitted, both resolve to `.minimium-local`; with `--map` or `--world-definition`, both default to `.minimium-local/map-<map-fingerprint>`. The persisted profile ID is a reconnect identifier, not account authentication.

The seeded demo worlds are used unless `--map PATH` or `--world-definition PATH` is supplied. Provide the same custom file to each client and the server. Welcome validates the map fingerprint and generator digest; these detect accidental mismatch and are not authentication.

The text client reports the environment clock's quality, estimated UTC date,
rate and revision when one of those status fields changes, and refreshes a usable
date at most every five seconds. It does not print a clock line each poll. The
desktop title shows the current quality and render UTC.
Explicit `--demo` mode uses a separate local UTC anchor and reports `LocalDemo`;
network sessions use only their server sample and monotonic probe estimate.

In the desktop Earth client, press F1 and enter `map_telepo`. The command opens
the `M` overview; click a land point to request a server-authoritative teleport.
Drag to rotate the globe, scroll to zoom (down to 1.08 planet radii), and press Escape to cancel selection.
Ocean destinations are rejected. The server places the player above the same
collision triangles used by movement and reports success or failure to the client.

Both frontends use one connection controller. It starts a background connector worker for DNS, sequential remote address attempts, or local server startup. The 10-second attempt timeout bounds when the front end accepts the result; blocking resolver or startup work cannot always be cancelled, so a timed-out/cancelled attempt remains busy until its worker exits. A replaced owned local server is cleaned up on a tracked worker before another attempt is allowed. After the connector returns a socket, the foreground client loop polls a shared nonblocking session; the session itself does not own a thread. The session sends Identify first, requires Welcome with the expected map fingerprint, queues app messages only when connected, and handles heartbeat timers. The initial Identify/Welcome exchange has a five-second client timeout; connected clients send pings every 10 seconds, disconnect after 20 seconds without a pong, and the server drops a client after 30 seconds without a valid game message.

### Text-client commands

The text client and in-window console use one startup command registry and parser. The interface, connection, gameplay, and environment modules each register their own commands. Both accept `help`, `host local [--threads 1|2|auto]`, `connect HOST:PORT`, `disconnect`, `fps show`, `net stats`, `time status`, `time scale DECIMAL`, `sky info`, `sky quality low|standard`, `ping`, `move X Z`, `jump`, `portal`, `map_telepo`, `fire`, `say TEXT` (alias `chat`), `name NAME`, `state`, `clear`, and `quit` (alias `exit`). `help` lists every registered command, and tab completion uses the same registry. `portal` requests a transfer when the player is near a portal; the desktop client also binds this action to E. `host local` starts or attaches to the local server configured by startup paths. It stores an owned server guard before later connection setup; only an owned child is stopped on exit. An attached server is left running. The terminal frontend renders state/events as text; the desktop frontend provides F1 console history and tab completion.

`fps show` reports that frame statistics are unavailable in the text client.
`sky info` and `sky quality` report that renderer diagnostics and quality controls require the desktop client. `time status` remains available and reports `Unavailable` when no server clock sample is present.

`time scale DECIMAL` (also `timescale DECIMAL`) changes the authoritative environment clock only when this client is connected to the local server process it started. Examples: `timescale 1` is real-time speed, `timescale 0.1` is ten times slower, `timescale 1000` is a thousand times faster, and `timescale 1000000` is a million times faster. `timescale 0` pauses the environment clock. The accepted range is 0 (pause) or 0.1–1,000,000 with at most six decimal places. The command switches the server clock to TimeScale mode, broadcasts the new clock to connected clients, and persists it with the server's save. The console shows the server's response when it arrives. For an attached or remote server, use the authenticated `minimiumctl --data-dir PATH time scale DECIMAL` administrative command instead. At high rates the supported date interval (2000–2100) can be exhausted quickly; the server reports a clock fault and pauses the environment at the last valid date.

### In-window console commands

The desktop client requests a 1920×1080 physical-pixel window by default; the window manager may constrain it to the available display area. Press F1 to open or close the console. It uses the shared registry above and renders text in Roboto Mono. Arrow keys browse history; PageUp/PageDown or the mouse wheel scroll the console, Home jumps to the oldest lines, End returns to the newest lines, and Tab suggests canonical commands and aliases. Lock the mouse and left-click to fire. `fps show` reports frame interval and render-loop work summaries; `net stats` reports application bytes, latest round-trip time, and nearby-player count.

`sky quality low|standard` changes only local rendering quality: `low` disables shadows and `standard` enables one 1024×1024 local shadow map. If the adapter refuses that resource, the client uses `low` and `sky info` shows requested/effective quality and the fallback reason. `sky info` prints a versioned, on-demand snapshot of clock synchronization, observer and celestial values, CPU timings, actual environment draw calls, shadow coverage, and estimated environment GPU bytes. `time status` prints the current session's read-only clock sample and synchronization state. `time scale` uses the owned server's private administrative endpoint and is unavailable for attached or remote sessions.

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
