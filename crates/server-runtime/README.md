# minimium-server-runtime

## Purpose / Non-goals

Owns the authoritative server lifecycle, world registry, tick, networking coordination, admission and persistence composition. It consumes fixed-domain server, transport, storage, environment, protocol, world and sector diagnostics loggers supplied by the host application. It enforces server-granted principal permissions at mutating input admission. Movement inputs carry intent only; a per-player server-owned policy supplies speed, acceleration and flight to the shared integrator. Privileged policy changes require the matching permission and are returned only to the requesting player. It is headless and has no GPU or window role.

## Public API

`run(args, process_environment, composition)` handles offline configuration commands from caller-captured environment values. Applications can use `RuntimeServerConfig::parse_with_environment` followed by `run_with_config` to resolve data, log and content paths before runtime startup. Runtime configuration parsing does not read process environment or working-directory state. The runtime retains the resolved log directory for telemetry integration. The application supplies a world-composer callback, combat values and modules through `composition`; the runtime does not choose sample content.

## Dependencies

Coordinates world, simulation, environment, protocol, transport, storage, platform, telemetry, assets, procgen and the neutral `minimium-gameplay` registration contracts. `composition.rs` defines neutral world, initial-entity and combat data accepted from the host application. On a new world, initial entities are validated against registered component schemas and committed through the snapshot-budgeted entity batch; the persisted initialization marker prevents reseeding after restart. Sample-game code is not a production dependency. Astronomy and GPU crates are forbidden.

## Data flow

The process validates startup inputs and reads world.seed before invoking the application world composer. The application owns saved planet and snapshot seed discovery and requests randomness only when no seed is available. Modules and their exact component schemas are registered before `PlayerStore::load`; the storage parser validates saved component types and versions before publishing `LoadedState`. The runtime restores the complete entity set, counters and initialization markers into staged world stores, then applies bounded network inputs on the owner lane. Workers publish typed results back to the runtime; immutable, revisioned save snapshots cross into storage.
The world registry validates every composed manifest before publishing an instance:
generated content requires revision 5 and the matching generator digest; authored
content must additionally match the materialized map fingerprint.

The gameplay context's targeted component lookup borrows a blob from the
world-scoped entity store, so repeated interaction events do not enumerate the
whole world on the owner lane.

## Ownership and execution

`ServerRuntime` owns mutable authoritative state on one lane. Worker threads do not mutate active worlds. Signal, admin `server stop`, and host-owner shutdown converge on the shared `ShutdownRequest`. Shutdown closes game and admin admission, rejects pending admin commands, then keeps the owner lane frozen while bounded workers stop and the final snapshot is captured and persisted. A single absolute deadline is 30 seconds from lifecycle start; worker, save, endpoint/status work has the first 28 seconds and the host diagnostics flush uses the remaining two seconds. A timeout is reported as failure and does not mean blocking operating-system I/O was cancelled. `Attached` client sessions never stop the server; only an `OwnedServer` sends graceful stop and reaps its child.

`runtime/operations.rs` keeps the loop order explicit and delegates autosave scheduling/capture to `poll_autosave`. `lifecycle.rs` coordinates admission closure, bounded admin drain, client socket closure, worker stop, final snapshot capture, persistence shutdown, endpoint cleanup and shutdown status in that order. The shared request carries the absolute diagnostics deadline back to the process host. Storage owns snapshot serialization and worker status; the runtime supplies the live world data and final snapshot. Persistence timeout/error and unfinished worker IDs propagate to the process result; the last atomically replaced snapshot remains intact.

`WorldRegistry` is the stable runtime façade for world instances. Its `membership.rs` module owns membership and epoch changes, `transfers.rs` owns staged and committed transfers, and `accessors.rs` owns read-only queries and save snapshots. Constructors and entity, sector, environment, and diagnostic operations stay in their dedicated modules.

Gameplay systems receive the owner-lane simulation tick, read-only component/event views, and a per-world New/Restored lifecycle during Startup. Startup runs once after each world is restored and before initial entities are marked initialized; saved initialization markers prevent portal reseeding. Systems return bounded effects. The runtime rejects a stage if the effect buffer limit or any entity, snapshot-budget, world-context, target-placement, or transfer-ID check fails. Entity stores, replication journals and snapshot budgets publish together only after batch validation. Transfer requests are preflighted with the same stage before entity effects commit.

`sector_streaming.rs` coordinates the per-tick pipeline. `sector_streaming/requests.rs` collects read-only player and transfer requests; `tickets.rs` owns retain/pin/release reconciliation; `workers.rs` dispatches generation to a fixed worker pool with a 64-job bound and installs up to eight completed results per tick; `readiness.rs` evaluates collision-ready players and transfers; and `collision.rs` rebuilds changed instances from installed payloads. Worker count defaults to `min(max(available_parallelism - 1, 1), 4)` and `--sector-workers` or `server.sector_workers` accepts 1..16 or `auto`.

Persistent sector replacements validate new totals and reserve the exact encoded record size before publication. Startup restores the complete sector set with staged state and budget copies, so a rejected restore leaves both unchanged. Startup also restores component entities against the already-registered exact component schemas using staged per-world stores, entity counters and content-initialization markers; a rejected entity restore leaves the live registry unchanged. All live entity spawn, update, component removal and despawn effects use one staged batch path that reserves encoded record bytes and aggregate raw component payload bytes before publishing the world stores, journal and budget together. A failed batch changes none of those states. The saved entity list is recaptured from the current registry on every autosave and final shutdown capture, including entities with zero components.

Entity replication keeps one immutable pending image or delta per client, bounded to 256 visible entities and the shared 4 MiB component-payload cap. Only components whose own visibility rule is satisfied enter that client's image; persistent entities with no visible replicated components have no client replica, and losing the last visible component despawns that replica. The server retains the 4 MiB change journal and client-specific acknowledged revisions; journal overrun or a 30-second pending sync restarts from a full image. Per-client entity-sync output is charged by exact encoded frame lengths against 64 KiB per replication tick, while `queue_encoded_frame_with_reserve` leaves one maximum server frame for control traffic. Client output, staging entities, components and payload bytes have independent limits; successful end commits atomically before its ack is queued.

On committed transfer the server clears both pending and already acknowledged
entity sync state, forcing a full image in the new world even when no sync was
pending at the moment of transfer.

Final capture checks the shared work deadline both before and after the
synchronous snapshot copy. Expiration is a process error; it does not start a
new persistence timeout.

## Invariants and errors

Bolid placement intersects the observer's gaze with the luminous atmospheric
shell. Physical heights, path lengths and speeds use the same Earth relief
scale as altitude commands and optical atmosphere; event duration remains in
seconds. Above the atmosphere it uses the near positive intersection; below it
uses the outward intersection. An orbital view missing the atmosphere rejects
the manual request with `server.bolid.no_atmosphere_in_view`. Event v19, seeded
appearance, automatic intervals and manual rate limits are unchanged.

Preserve current protocol/save compatibility, tick cadence, movement behavior, admission limits and shutdown ordering. Worker queue, protocol, storage, startup, environment-clock, command parsing and command execution errors retain stable codes at their boundaries. `ServerProcessError` preserves its stage and source; environment clock failures distinguish system-clock, supported-range, revision, saved-state and configuration faults while preserving adapter causes. Admin failure envelopes carry the originating command code. Command parsers return stable `CommandErrorCode` values directly; they do not infer a category by searching an error sentence.

## Commands

Startup accepts `--material-package PATH`, `--material-http-port PORT` (0 selects
an available port), and `--climate-scenario PATH`. Both material variants are
validated before admission. The HTTP endpoint serves only manifest-listed files;
it owns one bounded I/O worker and joins it at shutdown. Open the selected HTTP
port alongside the game port for remote clients. Climate initializes and persists
in `world.climate.ron` under the world data lock, outside snapshots; subsequent
starts restore it and reject a replacement initialization argument. The required
material set is session-wide and retained through existing world prepare/ready.
The corresponding properties are `world.climate.initial_scenario` (creation-only)
and `world.climate.relief_influence` (restart-required); climate is not mutable
through a live admin command.
`--environment-config PATH` maps to `world.time.initial_config`, a creation-only
property that initializes a new world's clock; saved snapshots remain authoritative.
`--dev` maps to `server.dev-mode=true`. Only this explicit mode makes the
application's development identity provider grant fly, movement-policy and
teleport permissions. Ordinary connections receive gameplay permission only;
the server checks privileged requests even when a client UI exposes the action.
The client `--dev` flag forwards developer mode only when starting its owned
local server; it cannot elevate a connection to a remote server.

Authentication is a nonblocking begin/poll/cancel provider contract with one
10-second attempt per connection. The runtime claims a profile only after a
provider returns a `Principal`; `PlayerStore` rejects an already-active profile.
Every mutating `WorldInput` is checked against its world context and Principal
permissions before queueing or changing server-owned movement policy. Permission
denials log the action and a stable process-local connection ID.
Admission also stops dispatching frames already decoded in the same network
poll after a close response (including `Quit`) moves the connection to
`Draining`; retained identity is used only for departure cleanup.

Server-owned handlers compose shared command metadata. Built-ins and modules declare argument type, optional/rest behavior, constraints, choices, units, and descriptions; help renders these descriptors. Built-ins reserve names before module registration, so module aliases cannot shadow host commands. Recognized built-ins with invalid arguments remain parse errors and are never reinterpreted as module commands. Game rules stay with their owning domain.

## Change guide

Change authoritative coordination and process lifecycle here. Reusable movement, collision, environment and storage logic belongs in their respective crates.

## Verification

The world registry exports only the transfer result consumed by runtime tick;
unused offer aliases and broad dead-code/import allowances are removed.

Run `rtk cargo check -p minimium-server-runtime -p minimium-server` and architecture/file-length checks. Runtime acceptance is deferred.


Protocol v16 admits context-bound `SetAltitude` requests through the bounded
world-input queue. The authoritative handler requires fly mode, a planet, no
pending transfer, finite 0..10000 km and valid destination coordinates. It samples
local collision terrain, applies Earth relief scaling, preserves angular location
and movement settings, resets velocity and replies with the precise destination.
The existing player-store/snapshot path records the relocation; saves are unchanged.

Protocol v19 carries the immutable v5 peak-relief definition, context-bound gameplay inputs and independent
ClimateScenario.relief_influence. Startup --climate-relief-influence accepts a
finite 0..1, applies it to the loaded climate scenario, and atomically persists
the changed scenario under the data lock. Other scenario values are preserved.
It is valid for existing worlds without rebuilding terrain. Omission restores
the persisted setting; legacy climate files default to 0.

Malformed explicit climate-relief numbers report the option name and accepted
range. Omitted options are handled by bootstrap; the server never substitutes a
default for an explicitly supplied invalid number.

Server startup configuration resolves defaults, schema v1 RON, mapped
`MINIMIUM_SERVER_*` environment variables, and CLI values in that order. Repeated
`--set KEY=VALUE` values share typed validation with legacy startup flags. Offline
`config validate` and `config show` run before runtime startup; JSON show output
is versioned, bounded to 64 KiB, and includes resolved values and source labels.
These commands do not lock storage, create worlds, write files, or bind listeners.

`server.autosave_interval` is the first live server property. Its change runs on
the owner lane, updates the autosave deadline, and leaves a save already in
progress untouched. Clock properties use the existing `TimeCommand` and
`EnvironmentClock` validation; world seed and generation identity reads come
from the active manifest.

Streamed sector geometry is captured as an immutable per-world snapshot and a
revisioned collision index is built by a fixed worker. Pending snapshots for a
world coalesce to the newest revision; stale results are discarded. Movement
and transfer readiness wait until the matching complete index is installed.

Movement acknowledgements are a safe resolved prefix. Admission tracks every
accepted sequence while it remains executable in the bounded world-input queue
or target-tick queue; rejection, queue pressure, same-tick replacement and
application/lateness resolve through one helper. The reported ack cannot pass
the lowest pending sequence, so a client never discards a sample that the
server may still apply. Snapshots also carry the current active movement intent,
so client replay starts with server-owned held input even when resolved samples
were rejected or replaced.
Successful map teleport clears scheduled movement and resolves its sequences.
Movement decoded later in the same input batch is discarded and resolved, so
an old sample cannot move the player immediately after the teleport.

The saved climate scenario is loaded and committed by the environment
configuration module. Material HTTP serving is owned by the `asset-io`
`ContentServer` adapter and receives only a validated package root plus the
application's fixed-domain asset I/O logger.

Admission accepts one bounded provider credential through
`AuthenticationProvider`; the provider returns a server-authored `Principal`
before content preparation or profile activation. The example server wires the
`dev.profile` provider, which treats the local profile ID as an identity claim
and is not an account or secure authentication system.

Atmospheric bolids are server-timed transient events. The server chooses a descending 80–110 km to 35–70 km path and a duration consistent with its speed. The live server.sky.bolid_interval property defaults to 120..300 seconds and accepts off; automatic events are usually faint. spawn_random_bolid always chooses bright class 2 near the observer's view through the admin endpoint, with seeded trajectory/material variation, and retains its five-second cooldown.

Manual placement samples at most 49 rays for each endpoint shell inside a
12-degree cone around the reported gaze. It selects a 20..300 km physical
chord whose radial velocity remains negative through the endpoint. This keeps
the complete initial path inside the minimum supported 30-degree vertical FOV
without changing v19. A view with no valid descending atmospheric chord returns
an explicit placement error; automatic placement is unchanged. Lengths and
speeds use the same Earth relief scale, so flight time remains physical seconds.

`spawn_random_bolid [N]` accepts 1..64 (default 1). The owner lane stores at most
one transient `bolid::shower::Shower`: remaining count, observer/context and two
monotonic deadlines. One first event is immediate; remaining bright events use
seeded clustered 0.9..8-second gaps and wider framing. A busy shower rejects a
second request; the existing five-second manual cooldown applies at admission.
No tick catch-up burst or separate timer thread is used. Observer/context loss,
placement/delivery failure or ten-minute expiry cancels remaining members.
Automatic births due during a series are rescheduled without emission. No wire
or save-format change. Runtime visual acceptance remains deferred.
