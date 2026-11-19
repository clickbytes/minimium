# Command and option reference

This page documents the current parsers and dispatchers. The [E051 compatibility matrix](console-api/compatibility.md) maps these spellings to the planned public operations. For operational sequences, see [prototype procedures](prototype.md); for metric meaning and limitations, see [performance policy](performance.md).

| Program | Options and commands | Defaults and parsing |
| --- | --- | --- |
| Server | `run`, `--config`, `--set`, `--seed`, `--planet-config`, `--environment-config`, `--threads`, `--max-players`, `--bind`, `--game-port`, `--data-dir`, `--map`, `--world-definition`, `--autosave-seconds`, `--dev`, `--headless`, `--help`/`-h`, `--version` | One lane, 512 players, loopback, port 0, `.minimium`, seeded generated worlds, 300 s autosave. Threads accept `1..=64` or `auto`. `--environment-config` initializes a new world's clock only; it is rejected when world data already exists. |
| Control | `--data-dir`, `--help`/`-h`, server commands below | `.minimium`; help is handled before command parsing. Only the first `--data-dir` is consumed. |
| Client | `--connect`, `--map`, `--world-definition`, `--profile-dir`, `--server-data-dir`, `--threads`, `--sky-quality`, `--demo`, `--console`, `--require-earth`, `--config`, `--server-set`, `--dev`, `--help`/`-h` | One local-server lane, standard sky quality, seeded demo worlds and `.minimium-local` paths; repeated options are errors. Demo conflicts with connect and console. `--require-earth` requires the desktop client and conflicts with demo. `--map` and `--world-definition` are mutually exclusive. |
| Load generator | `--address`, `--scenario`, `--clients`, `--seconds`, `--threads`, `--format`, `--scenario-id`, `--cpu-label`, `--cpu-mhz`, `--server-data-dir`, `--help`/`-h` | Address required; 100 clients, 10 s, heartbeat, auto workers and text output. Repeated option names are errors. |

Lane types remain application-owned: the server accepts `1..=64` or `auto`, the client accepts `1`, `2` or `auto` for a locally started server, and the generator accepts `1`, `2` or `auto` with at most two workers. Their different meanings and limits do not justify a shared policy type. Server CLI and help report the accepted `1..=64|auto` range.

## Operation syntax

Read a field with `world.time.scale`, assign with `world.time.scale = 1000`, or call a method with `world.time.status()`. A namespace lists its direct children. To inspect how the current host resolves a command, enter `ui.console.explain("timescale 1000")` in the client. The report contains the operation ID, normalized values, effect, required permission, source and recognized alias. It uses the host catalog without running the inspected command.

Use double quotes around text in canonical calls. Commas, parentheses, periods and equals signs inside that text are ordinary characters. Legacy chat retains its raw tail, including the equals sign in `say temperatura=20`. The F1 input limit is 128 UTF-8 bytes. The text terminal and developer presentation input accept 512 bytes.

## Server

~~~text
minimium-server run [--seed DECIMAL|0xHEX] [--planet-config PATH] [--environment-config PATH] [--threads 1..64|auto] [--max-players N] [--bind IP] [--game-port PORT] [--data-dir PATH] [--map PATH | --world-definition PATH] [--autosave-seconds N] [--dev] [--headless]
minimium-server --version
~~~

The defaults are one execution lane, 512 active players, loopback bind (`127.0.0.1`), an OS-selected game port (`0`), data directory `.minimium`, seeded planet/dungeon demo worlds, and autosave every 300 seconds. `--threads` accepts 1 through 64 lanes or `auto`; `--max-players` accepts 1 through 512. `--autosave-seconds` accepts 0 through 604800; zero disables periodic saves. `--headless` leaves the server in the foreground but disables terminal commands. Without it, terminal commands use the same command registry as the admin CLI. Game traffic binds to the selected IP; the administrative listener is loopback-only.

`--environment-config PATH` maps to creation-only `world.time.initial_config`. It reads at most 16 KiB of UTF-8 RON schema 1 with unknown fields rejected. It initializes `mode`, optional `initial_utc`, `scale_ppm`, and `offline_policy` for a new world. `real_time` requires no initial date and exactly 1x; `time_scale` uses the configured date or current UTC. Examples are [real-time.ron](../presets/environment/real-time.ron) and [time-scale.ron](../presets/environment/time-scale.ron). The option is rejected if `world.seed`, `world.planet.ron`, or `world.snapshot` already exists. A loaded save must use format v12; older and unknown versions are rejected without migration.

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
Close the client and use `.\start-earth.ps1 -RebuildWorld` to create a fresh
Earth world and player profile. `-ResetWorld` is an alias. The selected
`-DataDir` and `-ProfileDir` are preserved in sibling `.backup-*` directories
before new data is created; both must be separate directories and not in use.
Omit the switch on subsequent launches to resume that world.

For full resolution, download the GEBCO ice-surface netCDF archive, extract
`GEBCO_2026.nc`, install GDAL's `gdal_translate`, then run
`import-gebco.ps1 -GridPath GEBCO_2026.nc`. It writes eight checked tiles to
`third_party/gebco-2026` by default. A client started from the repository root
uses that package automatically; a client elsewhere sets
`MINIMIUM_EARTH_PACKAGE` to its local copy of the selected package.

The shared server command parser is owned by `minimium-server-runtime`; terminal and admin control use the same schema.

Server extensions use `EngineModule` registration in the compiled server. Each command declares a `CommandScope` (`ServerAdmin` for the built-in world diagnostics); the host selects the matching handler and context. The current `world` module registers `world list`, `world inspect ID`, and `entity list`, plus a startup diagnostic system. This is source-level registration, not a command that loads modules at runtime. Client commands use a separate parser registry whose connection, gameplay, interface and environment modules own their command specs and parsing.

Up to eight queued server commands execute at a tick boundary before world inputs and simulation. `world save` acknowledges queuing, not completion of the disk write; `stop` enters controlled shutdown after the current tick.

`--map PATH` loads a static map; `--world-definition PATH` loads a versioned manifest and authored dungeon blueprint. They cannot be combined. The server validates either file before binding. See [world contracts](worlds.md) for the authored definition format. `--game-port 0` asks the OS for an available port; a fixed port is useful when connecting explicitly. The server writes its selected game/admin endpoint into its data directory. Keep the directory private to one server process; an exclusive OS file lock prevents a second server from owning the same directory.

To include allocation counters in `metrics show`, enable the server feature at build/run time:

~~~text
cargo run -p minimium-server --features allocation-metrics -- run --data-dir .minimium
~~~

## Developer digging

The catalog routes `dev inventory tools` to gameplay. This operation idempotently grants a shovel and
pickaxe when the server allows developer actions. Keys `1` and `2` equip them;
`0` stows the tool and restores shooting. Left mouse digs within 3 m with a 0.6 m
brush, up to twice per second. Shovels remove soil; pickaxes also remove rock.
Confirmed inventory enables the digging HUD and an equipped low-poly viewmodel.
The colored aim/brush outline is a local first-hit/material preview; the last
server result remains visible separately. Geometry join errors and blocking
objects/players are distinct refusals. Both endpoints must be rebuilt together.

## Server commands

The same commands are available through the server terminal (unless `--headless`) and `minimiumctl`:

| Command | Meaning |
| --- | --- |
| `help` | Print the command list. |
| `status` | Report running state, active players, configured lanes, ticks, accepted/rejected clock probes, environment-sync messages, save state, player cap, map fingerprint, tick/snapshot intervals, autosave interval, protocol version, and save version. |
| `players list [CURSOR]` | List a bounded page of connected players. Pass the returned cursor to continue; positions are live values on each page. |
| `world list [CURSOR]` | List a bounded page of worlds. Pass the returned cursor to continue. |
| `entity list [CURSOR]` | List a bounded page of entities and players in stable world/kind/ID order. Pass the returned cursor to continue. |
| `entity inspect WORLD ENTITY [CURSOR]` | List a bounded page of the entity components; pass the returned cursor to continue. |
| `metrics show` | Print runtime, network, queue, collision, memory, process, and optional allocation metrics. Field names are documented in [performance policy](performance.md). |
| `world save` | Capture players, entities, sectors and the clock together, then queue one v12 snapshot on the persistence worker. A successful reply means queued, not completed; use `status` to inspect `save_pending` and `save_last`. A second save is rejected while one is pending. |
| `stop` | Request controlled shutdown. The server saves a final snapshot, reports the final result, and releases the data-directory lock. |
| `time status` | Show the committed tick and UTC, requested and achieved rates, pause state, limiting reason, clock revision, session, offline policy and observed real-time drift. |
| `time mode real_time` | Anchor to current server UTC at 1x. |
| `time mode time_scale` | Continue from the current world date, preserving its TimeScale rate or using 1x when switching from RealTime. |
| `time set RFC3339_UTC` | Set a UTC date and switch to TimeScale; preserve an existing TimeScale rate or use 1x. |
| `time scale DECIMAL` | Request a 0.1–1,000,000x rate and switch to TimeScale; zero pauses the game at a committed tick. |
| `time offline pause` / `time offline advance` | Set the restart policy persisted in save v12. `pause` resumes the saved world date; `advance` advances only the calendar by the nonnegative downtime multiplied by the saved scale. |

Rates accept decimal digits and an optional dot with one to six fractional digits, parsed exactly as ppm. Dates accept RFC3339 UTC with `Z` or `+00:00`, zero to three millisecond digits, and no leap second. Every accepted mutation increments the clock revision; invalid input is rejected without applying a change. Physical simulation, cooldowns, heartbeat and deadlines continue in real time.

The snapshot for `world save` captures the committed world at the command's execution point, before that tick applies its pending game inputs. A queued acknowledgement does not mean inputs still waiting for that tick are included. Controlled `stop` applies the pending input batch before taking its final snapshot.

`minimiumctl` syntax:

~~~text
minimiumctl [--data-dir PATH] <help|status|players list|metrics show|world save|stop|time ...>
~~~

The data directory defaults to `.minimium`. `minimiumctl stop` waits up to 120 seconds for the captured instance to finish, verifies its private owner marker, checks the legacy `server.shutdown` result, and waits for lock release. It exits with an error if the final save or shutdown failed. The legacy result body remains `<admin-port>\tOK` or `<admin-port>\tERR\t<message>`; the separate private owner sidecar binds that result to the instance. Neither file should be treated as a public credential store.

## Client

### Command domains and movement

The graphical and terminal consoles share command parsing and usage metadata.
Enter a domain (for example, `movement`, `network`, `sky`, or `weather clouds`)
to list its commands. The graphical console displays dim usage hints while typing,
including when a complete command name has no arguments yet. Invalid arguments
show the registered syntax, allowed values and units before any network request.

Canonical domains are `movement`, `network`, `world`, `combat`, `social`,
`camera`, `sky`, `time`, `weather`, `diagnostics`, and `console`.
`help` lists all registered usage. Existing registered aliases remain available;
Tab completes canonical names and aliases.

| Command | Behavior |
| --- | --- |
| `movement fly [on|off]` (alias `fly`) | Enables/disables flight with noclip; no argument reports settings. |
| `movement speed [0..10000]` | Shared walk/fly maximum speed in m/s; default 5. |
| `movement accel [0..10000]` | Shared walk/fly acceleration and braking in m/s�; default 30. |
| `movement move X Z` | Tangent movement direction, each component from -1 to 1. |
| `movement jump` | Request a jump and report the physical resolution. Flight mode returns a no-op. |

Speed and acceleration accept finite decimal values, inclusive of both limits.
Omitting their value reports current settings. Speed zero stops controlled movement;
acceleration zero disables acceleration and braking (the speed cap still applies).
The settings belong to the client connection controller and apply to both modes.
They are not saved in the world. Changes require a connected session.
The server validates and integrates movement using the same model as prediction.

In the graphical client, fly uses WASD horizontally, Space to rise and either Ctrl
to descend. Fly disables gravity and collision, including terrain/water traversal
restrictions. Turning fly off restores walking collision and gravity at the current
position. Close the console to control movement with the keyboard.


~~~text
minimium-client [--connect HOST:PORT] [--map PATH | --world-definition PATH] [--profile-dir PATH] [--server-data-dir PATH] [--threads 1|2|auto] [--sky-quality low|standard] [--demo] [--console] [--config PATH] [--server-set KEY=VALUE] [--dev]
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

Both frontends use one connection controller. It starts a background connector worker for DNS, sequential remote address attempts, or local server startup. The 10-second attempt timeout bounds when the front end accepts the result; blocking resolver or startup work cannot always be cancelled, so a timed-out/cancelled attempt remains busy until its worker exits. A replaced owned local server is cleaned up on a tracked worker before another attempt is allowed. After the connector returns a socket, the foreground client loop polls a shared nonblocking session; the session itself does not own a thread. Protocol v19 authentication uses bounded provider credentials and requires a server-issued principal before gameplay; Welcome validates world context and content digests. The session queues app messages only when connected and handles heartbeat timers. connected clients send pings every 10 seconds, disconnect after 20 seconds without a pong, and the server drops a client after 30 seconds without a valid game message.

### Text-client commands

The text client and in-window console use one startup command registry and parser. The interface, connection, gameplay, and environment modules each register their own commands. Both accept `help`, `host local [--threads 1|2|auto]`, `connect HOST:PORT`, `disconnect`, `fps show`, `net stats`, `time status`, `time scale DECIMAL`, `sky info`, `sky quality low|standard`, `ping`, `move X Z`, `jump`, `portal`, `map_telepo`, `fire`, `social say TEXT` (aliases `chat` and `say`), `name NAME`, `state`, `clear`, and `console quit` (aliases `exit` and `quit`). `help` lists every registered command, and tab completion uses the same registry. `portal` requests a transfer when the player is near a portal; the desktop client also binds this action to E. `host local` starts or attaches to the local server configured by startup paths. It stores an owned server guard before later connection setup; only an owned child is stopped on exit. An attached server is left running. The terminal frontend renders state/events as text; the desktop frontend provides F1 console history and tab completion.

`fps show` reports that frame statistics are unavailable in the text client.
`sky info` and `sky quality` report that renderer diagnostics and quality controls require the desktop client. `time status` remains available and reports `Unavailable` when no server clock sample is present.

`timescale` is the client alias for `time scale` on the owned local server. For a remote server use the authenticated `minimiumctl --data-dir PATH time scale DECIMAL` command. The accepted values are zero or 0.1 through 1,000,000 with up to six decimal places. A request for `1000` asks for one thousand game seconds per real second. In `time status`, `rate` gives the requested multiplier and `achieved_rate` gives the measured multiplier. At zero, world simulation and effects stop at the confirmed tick. Camera, UI, chat, joining, saving and administration remain available. If the next tick would exceed the UTC range, set a supported date and resume.

### In-window console commands

The desktop client requests a 1920×1080 physical-pixel window by default; the window manager may constrain it to the available display area. Press F1 to open or close the console. It uses the shared registry above and renders text in Roboto Mono. Arrow keys browse history; PageUp/PageDown or the mouse wheel scroll the console, Home jumps to the oldest lines, End returns to the newest lines, and Tab suggests canonical commands and aliases. Lock the mouse and left-click to fire. `fps show` reports frame interval and render-loop work summaries; `net stats` reports application bytes, latest round-trip time, and nearby-player count. Registered command descriptors define argument types, optional/rest behavior, choices, constraints, units, and descriptions; help and enum completion use this metadata.

`sky quality low|standard` changes only local rendering quality: `low` disables shadows and `standard` enables one 1024×1024 local shadow map. If the adapter refuses that resource, the client uses `low` and `sky info` shows requested/effective quality and the fallback reason. `sky info` returns a bounded typed prepared sky report with observer, quality and Sun/Moon values. `time status` prints the current session's read-only clock sample and synchronization state. `time scale` uses the owned server's private administrative endpoint and is unavailable for attached or remote sessions.

The desktop console accepts `fov` to display the current vertical field of view and `fov DEGREES` to set it from 30 to 110 degrees (default: 70). `show_celestial_names true|false` toggles on-screen labels for the Sun, Moon, visible catalog bodies, and up to 32 bright stars (default: false). The star display uses a fixed 1000-star HYG catalog. Both settings are local to the current client session.

### Developer stdin

With `--dev --dev-stdin`, the desktop host reads bounded stdin commands as soon
as the host exists, including while the world or presentation is loading. It
uses the same typed parser/executor as the console and text client, but prints
its own output and never edits console text, history or IME preedit. The
host-loop receiver accepts at most one queued line per iteration. Help, quit,
host diagnostics and operations that need no world remain usable while loading.
Scene-bound operations return an immediate typed unavailable reason if their
active-world, CPU-readiness or matching-presentation requirement is not met;
they are not queued for automatic execution after a transfer. The stdin worker
accepts at most 512 bytes per line and uses a 32-line bounded channel; longer
lines are logged and discarded. `--connect` and the terminal client do not
enable developer stdin.
`map_telepo` opens the Earth overview only for a connected, CPU-ready Earth
world with a matching scene; otherwise developer stdin prints its typed
unavailable reason without arming the map action.
`spawn_random_bolid [N]` also requires the active matching scene before it
stages the existing gaze/admin request; unavailable input is not retained.

`weather status` returns a typed prepared weather publication report with revision, cloud and region counts, build time, sample age and GPU memory estimate.

`weather clouds debug off|regions|layers|wind|kinds|detail` controls the graphical client's regional grid, source-floor colours, eastward wind arrows, family colours and voxel detail display.

`weather clouds detail 2|3|4|5|6` selects the preferred maximum recursive voxel depth N (default 6). Thin layers retain their minimum resolution. Budgets may simplify geometry further without changing cloud IDs. These commands are registered for help and completion; the terminal client reports that graphical weather controls require the graphical client.

See [regional weather](weather.md) and [clear-air atmosphere](atmosphere.md). The clock and astronomical lighting remain independent of the weather's fixed seasonal profile.

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
The typed property grammar is get KEY, set KEY VALUE, reset KEY, list [PREFIX], and help [COMMAND|KEY]. The first client property is client.camera.fov (decimal degrees, 30..=110, default 70, live, local). set client.camera.fov 85 and legacy camera fov 85 share the same domain validator; reset client.camera.fov clears the session override and returns to the default. get reports requested/effective/default/source. Client preferences use an explicitly selected schema v1 RON file and config validate/show run offline. config save PATH atomically writes explicit file entries plus eligible session changes; defaults and CLI values are not copied.


## Configuration and property contract

Property syntax is `get KEY`, `set KEY VALUE`, `reset KEY`, `list [PREFIX]`,
and `help [COMMAND|KEY]`. Descriptors provide the value type, unit, default,
constraints, owner, permission, lifecycle, persistence, and reset support. Domain
owners still validate values and apply state changes.

Startup precedence is default, RON file, explicitly mapped environment, then CLI.
Session overrides apply only to live properties. A property read reports requested,
effective, default, source, and any fallback reason. Reset removes the session
override and returns to the resolved startup value; it does not edit the RON file.

Client properties available to the text and desktop consoles are:
- `client.camera.fov`: finite degrees in 30..=110, default 70.
- `client.input.mouse_sensitivity`: finite rad/pixel in 0..=0.1, default 0.0025.
- `client.graphics.sky_quality`: low|standard, default standard.
- `client.sky.names` and `client.sky.demo`: true|false.
- `client.weather.cloud_detail`: integer 2..=6, default 6.
- `client.debug.atmosphere`, `client.debug.terrain`, and `client.debug.clouds`:
  enum values are listed by `help KEY`.
- `player.movement.fly`, `player.movement.speed`, and
  `player.movement.accel`: local queued input; numeric range 0..=10000.
- `world.time.mode`, `world.time.scale`, and
  `world.time.offline_policy`: live owned-local-server properties. UTC and
  revision are read-only replica values in clients.

The client does not expose `server.*`, world climate, or world identity properties.
Those are server startup or administrative settings. Movement reads describe local
input and queue acceptance, not server confirmation. Remote clients cannot change
server clock state.

The client accepts `--config PATH` and repeatable `--set KEY=VALUE` for client
preferences. `config validate`, `config show`, and `config save PATH` are
offline commands. Save writes only persistent selected settings using atomic file
replacement. Server startup accepts `--config PATH` and repeatable
`--set KEY=VALUE`; `config validate` and `config show --sources --format json`
run before runtime effects. Server threads accept 1..64 or auto; loadtest keeps
its own 1|2|auto range and has no config file or live settings.
Server administration also exposes `world.climate.initial_scenario` as
creation-only and `world.climate.relief_influence` as restart-required; neither
can be changed live.

The local client accepts `--server-config PATH` and repeatable
`--server-set KEY=VALUE`. It resolves these with the sibling server's offline
JSON view before launch. Existing servers reject explicit startup settings that
would be ignored. `--server-data-dir PATH` selects the server discovery target.
The admin CLI parses `--data-dir`, help, and version before reading endpoint
state; `stop` succeeds only after the captured server instance confirms shutdown.

Legacy command aliases map to the same property handlers: `camera fov` and
`fov` map to `client.camera.fov`; `fly` maps to `player.movement.fly`;
`movement speed` and `movement accel` map to their respective
`player.movement.*` properties. Time mutations remain `time set`, `time mode`,
`time scale`, and `time offline` actions through the owned local admin channel.

## Atmospheric bolids

The server admin command `spawn_random_bolid` creates an immediate bright class-2 bolid above an active planet observer, with a random trajectory and seeded material/breakup appearance. It needs no coordinates because the desktop client sends its current view direction. Manual requests have a five-second cooldown. `get server.sky.bolid_interval`, `set server.sky.bolid_interval 120..300`, `set server.sky.bolid_interval off`, and `reset server.sky.bolid_interval` control automatic night-time events. The startup setting also accepts `--set server.sky.bolid_interval=MIN..MAX` or `off` (10..=3600 seconds). Events are temporary visual effects and are not saved.

The desktop F1 console and text client also accept `spawn_random_bolid` with no arguments. It uses the owned local server admin endpoint. If entered while the desktop client awaits Welcome, the command is queued until the world and sky view are ready; the server reply appears in the console. Attached or remote servers require their own administration channel.

## Local cosmetic meteors (E047)

Commands: sky meteors status; clear; spawn PRESET [seed]; seed VALUE;
debug off|light|occlusion|paths|budget. Seeds are decimal u64 (0..18446744073709551615).
Help/completion list the presets; invalid arguments return typed errors.
Twenty families: ablation, burnout, erosion, droplets, irregular, single_split,
asymmetric_split, multiple_split, cascade, dustball, hard_core, single_flare,
multiple_flares, terminal_flare, airburst, remnants, dark_flight, debris,
low_flare, grazing. Legacy faint/bright/fireball/fragmenting remain.
Each family selects initial mass, speed, material, altitude and entry angle in
renderer/meteors/presets.rs; the same CPU solver determines the outcome.
Names do not guarantee an animation. Droplets use grain packets; dark flight
retains unlit mass. There is no physical implosion category.

Examples: sky meteors spawn airburst 42; sky meteors debug paths;
sky meteors debug occlusion; sky meteors debug off. Light/paths/budget print
console snapshots; occlusion shows selected source cloud transmission until off.
Legacy debug light on|off|transmission remains. No observatory HUD.

| Property under client.sky.meteors | Default / range | Persistence |
|---|---|---|
| enabled | true / boolean | preference |
| quality | standard / low,standard | preference; capped by effective sky quality |
| sound | false / boolean | preference; desktop-audio feature required |
| rate | 1 / finite 0..10 | session only; 0 pauses automatic births |
| seed_override | none / none or decimal u64 | session only |

Use get/set/reset/list and --set as for other client properties. At rate 1 the
automatic interval is 120..300 seconds; spawn never changes rate or world time.
Enabled=false clears active effects/audio and rejects manual spawn. Reset restores
startup settings without editing the config file; config save persists preferences
only. MINIMIUM_METEORS=0/1 and MINIMIUM_METEOR_SEED use config precedence.
client.audio.meteors is a compatibility alias for client.sky.meteors.sound;
client.audio.meteor_volume remains session-only 0..1. The terminal parser accepts
these commands and reports graphics unavailable without creating a GPU.

## Irregular server bolid shower

`spawn_random_bolid` emits one bright bolid as before.
`spawn_random_bolid 20` starts 20 bolids: the first immediately, the remaining
ones at seeded irregular 0.9..8-second intervals (usually about a minute or two
for 20). N is an integer 1..64; omitted N means 1. No duration argument is needed.
Showers scatter paths within a 24-degree cone around the current view instead
of the single-event 12-degree cone; narrow FOV can clip parts of a shower.

One server shower can run at a time; a new request while it runs returns
server.bolid.shower_busy. The five-second manual-request cooldown remains;
scheduled members are not new admin requests. Automatic events are skipped when
due during a shower and rescheduled normally. The tick emits at most one member
at a time and never catches up missed members in one burst. No sleeping thread,
world-time change, protocol change or persistent queue is added. The series is
pinned to its initial observer/context; loss/transfer of that observer, stale
sky view, failed atmospheric placement/delivery or a ten-minute expiry cancels
remaining members. Completed events remain subject to existing renderer caps.

This extends the existing server bolid demonstration, independently of E047
client-local `sky meteors spawn PRESET [seed]`. It requires an owned local server
from a client or an existing authorized server admin endpoint/console.

## Shared simulation clock

On an owned server, `world.time.scale = 1000` returns a submitted request ID. The matching owner_committed reply records the clock revision, requested 1000x scale, effective setting, tick and calendar epoch. `world.time.status()` reports the latest session sample, its age and the achieved ppm measured from completed ticks. Joining a running server, including localhost, returns client_runtime.admin_control_unavailable on a write. Administrative `get world.time.scale` reads the owner. `time set 2030-01-01T12:00:00.123Z` and `world.time.set(1893499200123)` commit through the same operation. `set_time_local 1200` uses the active Earth observer at execution. See [ADR 0016](adr/0016-committed-simulation-time.md).

## Camera preferences

The FOV factory default is 70 degrees. With 80 in the preferences file, setting FOV to 95 and resetting restores 80 from the file. If the command line selected 90 at startup, the same reset restores 90. Both operations report the settings base and revision. The renderer acknowledges that pair before publishing effective FOV. Use `client.camera.status()` for a presentation sample and `client.camera.look(45, -10)` to set yaw and pitch in degrees. `client.config.save("preferences.ron")` writes the persistent file layer and current session overrides.

## Movement policy

Speed, acceleration and flight are scoped to the session actor and current world. Each write sends only its field. The reply carries the original request sequence and context, and the approved policy revision or reason for refusal. Reads show the confirmed effective value and sample age, with pending requests reported separately. A paused client suppresses the request. Disconnecting before acknowledgement leaves the outcome unknown.

Jump admission reserves one console intent. Its final receipt carries the physical decision at the resolved tick. Multiple requests in a tick share one impulse decision. Device flight ascent remains a separate input.

Partial input uses the operation catalog. For example, completing `get ui.lo` edits only `lo`, and choosing a value in `set ui.locale p` preserves the key. Completing `client.cam|ra.fov = 90` at the marked caret preserves `.fov = 90`. An open call displays its registered argument types. Terminal and developer stdin can inspect this same analysis with `ui.console.explain("world.time.")`.

F1, developer stdin and terminal accept 512 UTF-8 bytes per line before trimming. Oversize lines report input_too_long. Tab applies a suggestion, Enter submits a complete line, and Escape dismisses suggestions before closing F1. Up/Down selects candidates or browses the 128-entry command history. Clear preserves command history. Use help NAME to inspect a field, method or its registered alias.

Use client.connection.stats() for total process traffic and the optional current session. player.state() reports prediction and the authoritative sample with its server tick and age. A connection attempt and a queued ping each have a later correlated receipt. Disconnect reports the session, cancelled attempt and local-process cleanup separately. Quit acknowledges the shutdown request.

Chat accepts 1..128 UTF-8 bytes without control characters. Player names accept 1..24 ASCII letters, digits or underscores. The fire receipt includes the direction and its source (camera or terminal default). Player action receipts identify their world input sequence and report when the transport confirms a complete frame write.

Portal, dev goto LAT LON, and set_altitude KM submit typed actor navigation. Goto accepts finite latitude -90..90 and longitude -180..180 degrees and prepares direction on the CPU, including terminal mode. The server decides world context, permission and target validity. world teleport/map_telepo only arms a desktop Earth-map click; the click submits a separate teleport request and the arm receipt does not claim teleport completion.

### Local presentation values and reports / Lokalne ustawienia i raporty

The get, set, and reset forms for client.input.mouse_sensitivity,
client.input.bindings, client.graphics.sky_quality, client.sky.names,
client.weather.cloud_detail, client.debug.atmosphere, client.debug.moon_flux,
client.debug.exposure_ev, client.debug.terrain, client.debug.clouds, and
client.sky.demo return typed property values. Requested, effective, startup,
default, source, and fallback reason stay separate. Reset restores startup.
F1 and terminal share parsing and validation. A text host can read preferences
and change input bindings; renderer settings require the desktop capability.

The seven presentation queries are client.render.frames.status(),
client.sky.status(), client.sky.bodies.list(), client.render.terrain.status(),
client.render.atmosphere.status(), client.weather.status(), and
client.weather.clouds.targets(). Existing fps show, sky info, sky bodies,
terrain status, atmosphere status, weather status, and dev clouds spellings
bind to them. Each returns a bounded typed record with available world identity,
clock revision, freshness, and source. Missing samples and stale world content
return Unavailable. GPU timing distinguishes unsupported, pending, and measured
samples; measured values include pass scope and age. Body results are capped at
128 rows and carry truncated.

Po polsku: odczyt, zapis i reset lokalnych preferencji zachowują osobno wartość
żądaną, efektywną i startową oraz przyczynę ograniczenia. Zapytania renderera
wymagają przygotowanej próbki; brak próbki zwraca Unavailable. Lista ciał
jest ograniczona i oznacza obcięcie.

## Typed local meteor and bolid preview operations (E051-17)

`get/set/reset client.sky.meteors.enabled|rate|quality|seed_override|sound` and
`client.audio.meteor_volume` return typed property values with requested,
effective, source and revision. The old `client.audio.meteors` spelling is an
alias of the persistent `client.sky.meteors.sound` preference. Config files may
contain either spelling; equal values normalize to one canonical setting and
conflicting values stop startup. An unavailable audio backend leaves the requested
preference intact and reports effective audio as unavailable.

`sky meteors status`, `spawn PRESET [SEED]`, and `clear` use the local presentation
owner. All 24 existing preset names remain valid; IDs and seeds are exact u64.
`sky meteors debug MODE` and `sky meteors debug light MODE` update session state
and return a typed property value. The light, paths and budget modes also attach
a bounded diagnostic snapshot; a property read does not run diagnostics.
Meteor status names presentation time and reports prepared context, source,
capability and uninstrumented GPU timing. Local active events are capped at eight.

`dev bolid transmission on|off` controls a local GPU diagnostic and can differ
from its requested value after resize. `dev bolid path ID KIND SEED DURATION_MS
X0 Y0 Z0 X1 Y1 Z1`, `dev bolid replay AGE [SEED [EVENT_ID]]` and `dev bolid
freeze AGE [SEED [EVENT_ID]]` require an event retained for the active world.
Age is 0..120 presentation seconds; path duration is 300..6000 ms. `dev bolid
status|fragments|lighting` return bounded typed reports with context and source.
A missing event or wrong context is an error. These commands never create a
server request, entity or save record. The server-owned `server.sky.bolid_interval`
controls automatic bolids in simulation seconds (off or 10..3600), while manual
server meteorid requests have a five-second real-time cooldown.

## Typed server configuration and administration (E051-18)

`get` and `help` expose `server.sector_workers` (auto or 1..16),
`server.fluid.world_cells` (1..262144), `server.fluid.server_cells`
(1..524288) and `server.dev-mode` (true/false) alongside the existing server
configuration fields. The two fluid budgets must also satisfy world <= server.
These fields require restart; `world.map`, `world.definition` and
`world.planet_config` are creation-only. Live `set/reset` does not change the
configuration file. `server.autosave_interval` is live, 0..604800 monotonic
seconds; reset restores the resolved startup value, including a file or CLI
value. Reads identify default/file/environment/CLI and any session override,
with configured and effective worker or port values separated.

`server.status()`, `server.metrics.status()`, `server.players.list()` and
`server.stop()` are typed owner operations. The player list defaults to 64 rows,
accepts `server.players.list(none, 1..256)` or a returned cursor, and carries
`next_cursor` in a bounded page. Legacy `players list [CURSOR]` retains a 256-row
default. Stop confirms that the owner committed the shutdown request; it does
not claim the process or save has finished. An oversized typed result is a
`response_too_large` error in the outer admin response too. Local admin access
is required, independently of the desktop renderer.

## Typowana konfiguracja i administracja serwera (E051-18)

`get` i `help` pokazują `server.sector_workers` (auto lub 1..16),
`server.fluid.world_cells` (1..262144), `server.fluid.server_cells`
(1..524288) i `server.dev-mode` (true/false). Budżet świata nie może
przekroczyć budżetu serwera. Te pola wymagają restartu; `world.map`,
`world.definition` i `world.planet_config` obowiązują tylko przy tworzeniu.
Zwykłe `set/reset` nie zmienia pliku konfiguracji. `server.autosave_interval`
można zmieniać na żywo w zakresie 0..604800 sekund czasu monotonicznego;
reset przywraca wartość rozstrzygniętą przy starcie, również z pliku lub CLI.
Odczyt podaje pochodzenie oraz osobno wartość żądaną i efektywną.

`server.status()`, `server.metrics.status()`, `server.players.list()` i
`server.stop()` zwracają typowane wyniki właściciela. Lista graczy domyślnie
ma 64 wiersze, dopuszcza limit 1..256 i zwraca kursor następnej strony.
Stara składnia `players list [CURSOR]` zachowuje limit 256. Stop potwierdza
przyjęcie żądania zamknięcia przez właściciela, a nie zakończenie procesu
lub zapisu. Zbyt duży wynik daje `response_too_large` także w zewnętrznej
odpowiedzi administracyjnej. Wymagane są uprawnienia administratora;
renderowanie nie nadaje takich uprawnień.
## World catalog and primary generation (E051-19)

`world.primary.generation.seed`, `.revision`, and `.digest` read the primary world's manifest, including when an actor is in a dungeon or a transfer is pending. The seed property reports configured `auto` or exact decimal/hex u128, configuration source and effective manifest seed. `world.seed`, `world.generation_revision`, and `world.generator_digest` remain aliases. Startup configuration accepts both `world.primary.generation.seed` and `world.seed`; conflicting values in one source fail. `--seed` and `MINIMIUM_SERVER_SEED` still work. Live seed writes are creation-only errors; revision and digest are read-only.

`world.instances.list(cursor?, limit=64)`, `world.instances.inspect(instance_id)`, `world.objects.list(cursor?, limit=64)`, and `world.entities.inspect(instance_id, entity_id, cursor?, limit=64)` use the server owner's world registry. Lists allow 1..256 rows and a 32 KiB page budget; legacy `world list`, `world inspect`, `entity list`, and `entity inspect` keep their existing spelling and 256-row page default. Object pages include both entities and players with explicit kind and world ID. Entity inspection returns component kind, schema and byte count without payload. IDs remain exact u128/u64. The 134-byte v3 cursor binds runtime, query, scope and membership revisions; changes to position and other values are read live on each page. A restart or structural membership change makes the cursor stale. The server checks admin scope before looking up IDs; a revoked admin request is denied before reading a page.

## Katalog swiatow i generacja primary (E051-19)

`world.primary.generation.seed`, `.revision` i `.digest` odczytuja manifest swiata primary rowniez wtedy, gdy gracz przebywa w dungeon lub transfer jest w toku. Seed pokazuje zadana wartosc `auto` albo dokladne u128, zrodlo konfiguracji i efektywna wartosc manifestu. Stare nazwy `world.seed`, `world.generation_revision` i `world.generator_digest` pozostaja aliasami. Konfiguracja startowa przyjmuje nowy klucz lub `world.seed`; sprzeczne wartosci w jednym zrodle sa bledem. `--seed` i `MINIMIUM_SERVER_SEED` nadal dzialaja. Zapis seeda na zywo daje blad creation-only, a revision i digest sa tylko do odczytu.

Metody `world.instances.list`, `.inspect`, `world.objects.list` i `world.entities.inspect` korzystaja z odczytu wlasciciela `WorldRegistry`. Listy maja domyslnie 64 wiersze, limit 1..256 i budzet strony 32 KiB; stare polecenia zachowuja domyslne 256. Lista obiektow obejmuje graczy i encje, z jawnym typem i ID swiata. Inspekcja encji zwraca wylacznie kind, schema i liczbe bajtow komponentu. ID pozostaja dokladnymi u128/u64. Kursor v3 ma 134 bajty i wiaze runtime, zapytanie, zakres oraz rewizje czlonkostwa. Wartosci takie jak pozycja sa odczytywane na zywo na kazdej stronie; restart lub zmiana czlonkostwa uniewaznia kursor. Kontrola uprawnien administratora poprzedza sprawdzenie istnienia ID.
## Authoritative bolid and impact receipts (E051-20)

`server.sky.bolid_interval` remains the live server property: `off` or `MIN..MAX` simulation seconds (10..3600, ascending). Reset restores the startup layer. `server.sky.bolids.spawn(count=1)` and `spawn_random_bolid [N]` share one owner, with count 1..64. The first delivered event returns `owner_committed`, its event/shower ID, actual instance, observer, WorldContext, tick, emitted=1 and remaining=N-1. `server.sky.bolids.status(shower_id)` (legacy `bolids status ID`) reports requested/emitted/remaining, event IDs and running/completed/partial/cancelled/failed state. One shower runs at a time; 64 terminal records remain for ten monotonic minutes. An expired or restarted ID is `untracked`. Gaps of about 0.9..8 seconds and a 600 second expiry use simulation time; manual cooldown is five real seconds. A changed observer context stops the remainder without changing the already emitted events.

`world.impacts.spawn_random()` and `spawn_random_meteorid` return `owner_accepted` with a reserved body ID and pending checkpoint. `world.impacts.status(instance_id, body_id)` (legacy `impacts status WORLD ID`) reports `owner_committed` only after journal acknowledgement and owner world publication, including a session-local durable ACK ordinal, or `failed` after a journal failure. The bounded volatile receipt ledger retains 64 records; restart or eviction is `untracked`. Target world, observer, context and ticks are explicit. The impact's later flight, crater and save lifecycle are separate. Client-local meteors remain visual only. F1 and terminal use the owned local admin channel; attached sessions cannot elevate. Desktop gaze is optional input to the server's observer selection, not an authorization requirement.

## Autorytatywne potwierdzenia bolidów i uderzeń (E051-20)

`server.sky.bolid_interval` pozostaje właściwością serwera: `off` albo rosnący zakres `MIN..MAX` sekund symulacji (10..3600). Reset przywraca wartość startową. `server.sky.bolids.spawn(count=1)` i `spawn_random_bolid [N]` wywołują tego samego właściciela dla N od 1 do 64. Pierwsze wyemitowane zdarzenie zwraca etap `owner_committed`, ID zdarzenia i serii, rzeczywistą instancję, obserwatora, WorldContext, tick oraz liczby emitted=1 i remaining=N-1. `server.sky.bolids.status(shower_id)` (`bolids status ID`) podaje stan, liczby i ID zdarzeń. Jednocześnie działa jedna seria; 64 zakończone rekordy są przechowywane przez dziesięć minut czasu monotonicznego. Restart albo wygaśnięcie daje `untracked`. Odstępy około 0,9..8 s i ważność 600 s używają czasu symulacji; ręczny cooldown to pięć sekund rzeczywistych. Zmiana kontekstu obserwatora kończy resztę serii bez usuwania wcześniejszych zdarzeń.

`world.impacts.spawn_random()` i `spawn_random_meteorid` zwracają `owner_accepted` z zarezerwowanym ID ciała i oczekującym checkpointem. `world.impacts.status(instance_id, body_id)` (`impacts status WORLD ID`) pokazuje `owner_committed` dopiero po potwierdzeniu journalu i publikacji świata przez właściciela albo `failed` przy błędzie. Rewizja journalu w raporcie jest lokalnym dla sesji licznikiem trwałych potwierdzeń. Ograniczony, ulotny rejestr obejmuje 64 rekordy; po restarcie lub wyparciu wynik to `untracked`. Raport zawiera rzeczywisty świat, obserwatora, kontekst i ticki. Późniejszy lot, krater i zapis są osobnymi etapami. Meteory klienta pozostają wyłącznie wizualne. F1 i terminal korzystają z administracji własnego lokalnego serwera; dołączona sesja nie otrzymuje uprawnień administratora. Dane wzroku z desktopu są opcjonalną próbką dla wyboru obserwatora, a nie warunkiem autoryzacji.

## E051-21: despawn encji / entity despawn

PL: Kanoniczne `world.entities.despawn(instance_id, entity_id)` i stare `entity despawn WORLD ENTITY` używają tego samego efektu świata. Argumenty są dokładnymi niezerowymi ID u128/u64; wymagany jest ServerAdmin. Wynik OwnerCommitted pojawia się dopiero po stage, rezerwacji snapshotu i publikacji EntityStore; zawiera rewizję oraz token producenta i sekwencję efektu. Odrzucenie chronionego komponentu albo zajętej rezerwacji nie usuwa encji. Admin nie dostaje portu ownera komponentu. Brak ACK po rozłączeniu ma stan UnknownAfterDisconnect; ponowne jawne wywołanie może dać UnknownEntity. Trwały zapis dyskowy pozostaje osobnym etapem.

EN: `world.entities.despawn(instance_id, entity_id)` and legacy `entity despawn WORLD ENTITY` use one world effect. Arguments are exact nonzero u128/u64 IDs and require ServerAdmin. OwnerCommitted follows staging, snapshot reservation and EntityStore publication; the reply carries the store revision and producer/sequence effect token. Protected components and reserved entities are rejected without removal. Admin access does not grant a component owner port. A missing acknowledgement after disconnect is UnknownAfterDisconnect; an explicit retry may return UnknownEntity. Disk fsync is a separate stage.

## E051-22: stan wody / water status

PL: `world.water.status(instance_id)` wymaga ServerAdmin i jawnego niezerowego ID świata u128. Zwraca ograniczony odczyt ownera tej instancji: capture_id, committed_tick, pending_target_tick, rewizję fluid i oddzielną persisted_revision, dirty, digest katalogu (32 bajty jako hex), wersje edits/remnants/volume, stan joba, failure/retry, checkpoint_due oraz liczbę i limit komórek. Spójność to OwnerCaptureWithPendingBoundary, nie pełny commit wszystkich subsystemów w tym samym ticku. Persisted_revision jest wartością WaterWorld, nie gwarancją fsync dowolnego backendu. Nieplanetarny świat to NotApplicable(NoHydrology), brak stanu to Unavailable, awaria to Failed; nieznany świat to UnknownInstance. Zapytanie nie uruchamia solvera, checkpointu ani geometrii.

EN: `world.water.status(instance_id)` requires ServerAdmin and an explicit nonzero u128 world ID. The bounded, per-instance owner capture reports capture_id, committed_tick, pending_target_tick, fluid and separate persisted revisions, dirty, the 32-byte catalog digest as hex, edits/remnants/volume versions, job state, failure/retry, checkpoint due and cell count/limit. Consistency is OwnerCaptureWithPendingBoundary, not a full cross-subsystem tick commit. Persisted revision is WaterWorld tracking state, not proof of fsync by any backend. A nonplanet world is NotApplicable(NoHydrology), missing state is Unavailable, failure is Failed, and an unknown world is UnknownInstance. The query starts no solver, checkpoint or geometry work.

## E051-23: prezentacja wody / water presentation

PL: `client.render.water.status()` jest lokalnym typowanym Query z capability Renderer. Received opisuje aktywną replikę: context, sync ID, fluid revision, digest katalogu, terrain edits/remnants/volume i region. Requested jest kluczem przyjętego żądania workera; Installed to klucz geometrii po udanej publikacji, z liczbą lokalnych vertices/triangles. Oba klucze zachowują instance/context, origin generation, source revision, water_level_hash, centrum i limit 96 m/4096 komórek. Hash poziomu nie jest monotoniczną rewizją. Stany Available, Loading, NoSnapshot, NotApplicable, Stale i Failed nie oznaczają pustego świata. W terminalu metoda zwraca Unavailable(renderer_missing), bez kontaktu z serwerem. Serwerowe `world.water.status(instance_id)` pozostaje odrębne.

EN: `client.render.water.status()` is a local typed Query requiring the Renderer capability. Received reports the active replica's context, sync ID, fluid revision, catalog digest, terrain edits/remnants/volume and region. Requested is the accepted worker request key; Installed is geometry after successful activation, with local vertex/triangle counts. Both keys retain instance/context, origin generation, source revision, water_level_hash, center and the 96 m/4096 cell limits. The water-level value is a hash, not a monotonic revision. Available, Loading, NoSnapshot, NotApplicable, Stale and Failed do not imply a globally empty world. Terminal returns Unavailable(renderer_missing) without contacting the server. Server `world.water.status(instance_id)` is independent.

## Persistence jobs (E051-24)

`server.persistence.save()` and `world save` capture all worlds, retained player profiles, the committed clock boundary and the excavation journal revision. A successful owner receipt means the immutable snapshot was queued, with an exact runtime-scoped `job_id`, snapshot revision, committed tick, journal revision, and world count; it does not promise disk completion. `server.persistence.status(job_id?)` reports the active and last known job and their manual/autosave/shutdown source. Matched file completion is `durable`, memory completion is `volatile`, and an injected backend without a stronger declaration is `backend_defined`. A second save while one is pending returns Busy without changing the first job. Unknown and expired IDs are different results. Jobs are metadata only: at most one active and 64 completed for ten monotonic minutes. Shutdown waits for an accepted job before queuing its final snapshot. A timeout or disconnect after acceptance does not cancel or replay the save.

## Zadania zapisu (E051-24)

`server.persistence.save()` i `world save` zapisują wszystkie światy, zachowane profile graczy, zatwierdzoną granicę zegara i rewizję dziennika wykopów. Potwierdzenie właściciela oznacza przyjęcie niezmiennej migawki do kolejki, z dokładnym `job_id` związanym z uruchomieniem, rewizją migawki, zatwierdzonym tickiem, rewizją dziennika i liczbą światów; nie obiecuje ukończenia zapisu na dysku. `server.persistence.status(job_id?)` pokazuje aktywne i ostatnie zadanie oraz źródło manual/autosave/shutdown. Dopasowane ukończenie plikowe jest `durable`, pamięciowe `volatile`, a backend bez silniejszej deklaracji `backend_defined`. Drugi zapis podczas oczekiwania zwraca Busy bez zmiany pierwszego zadania. Nieznane i wygasłe ID mają odrębne wyniki. Ledger przechowuje metadane najwyżej jednego aktywnego i 64 ukończonych zadań przez dziesięć minut czasu monotonicznego. Shutdown czeka na przyjęte zadanie przed dodaniem końcowej migawki. Timeout lub rozłączenie po przyjęciu nie anuluje ani nie ponawia zapisu.

## Measured clock progress and diagnostics (E051-25)

`world.time.achieved_scale` is a read-only `MeasuredRatePpm` value. Its exact U64 ppm comes from committed server tick progress; the requested `world.time.scale` is a separate configuration value. `get world.time.achieved_scale` reads the session sample in F1/terminal and the owner snapshot in admin. An absent sample is unavailable, and a stale sample keeps its quality label. LocalDemo and the client interpolation segment rate cannot supply authoritative measured progress. `world.time.diagnostics()` returns one bounded sample: clock and runtime session identity, committed tick, calendar epoch, clock revision, requested and achieved ppm, limit, source and sample age. Owner-only fault, correction and dirty revision are optional on a client sample. `time status` still has its prior result. There is no setter or reset for achieved scale.

## Zmierzone tempo zegara i diagnostyka (E051-25)

`world.time.achieved_scale` jest właściwością tylko do odczytu typu `MeasuredRatePpm`. Dokładna liczba U64 ppm pochodzi z postępu zatwierdzonych kroków serwera; żądane `world.time.scale` jest osobną konfiguracją. `get world.time.achieved_scale` czyta próbkę sesji w F1/terminalu, a w administracji migawkę właściciela. Brak próbki daje niedostępność, zaś stara próbka zachowuje etykietę jakości. LocalDemo ani tempo lokalnego segmentu interpolacji nie są autorytatywnym pomiarem. `world.time.diagnostics()` zwraca jedną ograniczoną próbkę: tożsamość zegara i sesji runtime, zatwierdzony tick, epokę kalendarza, rewizję zegara, żądane i osiągnięte ppm, limit, źródło i wiek próbki. Pola tylko właściciela: fault, correction i dirty revision, są opcjonalne w próbce klienta. `time status` zachowuje dotychczasowy wynik. Zmierzone tempo nie ma settera ani resetu.
