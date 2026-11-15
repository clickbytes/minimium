# minimium-server-runtime

## Purpose / Non-goals

Owns the authoritative server lifecycle, world registry, tick, networking coordination, admission and persistence composition. It consumes fixed-domain server, transport, storage, environment, protocol, world and sector diagnostics loggers supplied by the host application. It enforces server-granted principal permissions at mutating input admission. Movement inputs carry intent only; a per-player server-owned policy supplies speed, acceleration and flight to the shared integrator. Privileged policy changes require the matching permission and are returned only to the requesting player. It is headless and has no GPU or window role.

## Public API

`run(args, process_environment, composition)` handles offline configuration commands from caller-captured environment values. Applications can use `RuntimeServerConfig::parse_with_environment` followed by `run_with_config` to resolve data, log and content paths before runtime startup. Runtime configuration parsing does not read process environment or working-directory state. The runtime retains the resolved log directory for telemetry integration. The application supplies a world-composer callback, combat values and modules through `composition`; the runtime does not choose sample content.

## Dependencies

Coordinates world, simulation, environment, protocol, transport, storage, platform, telemetry, assets, procgen and the neutral `minimium-gameplay` registration contracts. `composition.rs` defines neutral world, initial-entity and combat data accepted from the host application. On a new world, initial entities are validated against registered component schemas and committed through the snapshot-budgeted entity batch; the persisted initialization marker prevents reseeding after restart. Sample-game code is not a production dependency. Astronomy and GPU crates are forbidden.

## Data flow

The process preflights snapshot and excavation journal versions before creating its data lock, composes and loads saved world state read-only, then acquires the lock and revalidates persistent format headers and world seed/planet inputs before any startup cleanup or writes. The application owns saved planet and snapshot seed discovery and requests randomness only when no seed is available. The world impact owner registers its ServerOnly body descriptor in the base component registry before application module schemas; the complete registry is ready before `PlayerStore::load`. The storage parser validates saved component types and versions before publishing `LoadedState`. The runtime restores the complete entity set, counters and initialization markers into staged world stores, then applies bounded network inputs on the owner lane. Live entity effects also stage ordered neutral changes alongside cloned world stores; storage prepares encoded-record and aggregate-payload reservations, and the owner commits both candidates in one operation before later effects run. Workers publish typed results back to the runtime; immutable, revisioned save snapshots cross into storage.

Administrative execution returns a typed success payload or a typed failure code and bounded detail. Built-in commands return structured reports and immutable owner snapshots; the shared command output adapter formats them for both terminal and admin responses. Only extension module text uses the bounded opaque text payload. The admin wire envelope, permissions and stable error codes remain owned by their existing adapters.

Registered module commands receive the neutral `ModuleCommandContext`, not the
owner command context used by persistence, clock and shutdown built-ins. The
module context lends bounded world, entity, player and component read pages and
accepts bounded `WorldEffect` emission. Pages cap at 256 rows; dynamic names and
component bytes cap at 1 MiB, with explicit truncation and distinct missing
world/entity errors. The built-in `world` module uses this port for world and
entity queries and emits `DespawnEntity` for `entity despawn`. Successful
handlers pass their batch through the same `commit_effect_batch` as gameplay
systems; handler errors discard their emitted effects. The reply carries the
typed `EffectBatchOutcome`, and its formatter reports retained results with
truncation explicitly marked at the existing output bound. Effect rejection makes the command response
an error or partial-effect error rather than a full success. A transfer's
Started result means its prepare was queued, not that the transition is ready
or committed.

Rendered command text, module text and failure details are capped at 8 KiB with a UTF-8-safe `[truncated]` marker. This leaves room for worst-case JSON escaping within the existing 64 KiB admin reply cap. List snapshots have separate row caps and report truncation explicitly; metrics client rows are capped at 512.

Property reads preserve their existing successful response for unknown keys and unavailable clock properties through typed `UnknownProperty` and `UnavailableProperty` query results. Mutation validation failures continue to use the established typed error codes.
The world registry validates every composed manifest before publishing an instance:
generated content requires revision 6 and the matching generator digest; authored
content must additionally match the materialized map fingerprint.

The gameplay context's targeted component lookup borrows a blob from the
world-scoped entity store, so repeated interaction events do not enumerate the
whole world on the owner lane.
Gameplay systems can also borrow `TerrainRead` for a planetary world. The runtime
builds it from the current manifest, prepared geography, crater snapshot and
committed volume store; the legacy crater snapshot accessor remains available.
Nonplanet worlds and failed terrain preparation return no read port.
Validated planet construction rejects missing or mismatched Earth geography;
optional spatial-query ports report unavailable on sampling failure. Water
catalog startup propagates `GenerationError` through its I/O adapter, and sector
preparation retains the typed generation cause before publication is considered.
Planet admission and transfer destinations validate the standing capsule against
that current signed field, retaining a saved tunnel position only while it is clear.

## Ownership and execution

`ServerSchedule` consumes the validated gameplay `SchedulePlan`. The fixed tick
dispatches its `BuiltinOperation` sequence, not a second handwritten ordering:
commands → bolids → impacts → excavation → shutdown gate → world inputs →
FixedUpdate handlers/effects → simulation → Environment handlers/effects →
PostSimulation handlers/effects → gameplay-event cleanup. Each stage retains
one bounded effect batch and its existing publication point. Command batches
still publish per command. A shutdown requested by commands or asynchronous
owners still stops at the gate after excavation, before world inputs or module
stages; tick completion metrics/deadlines remain after the full sequence.

Simulation remains one owner operation: synchronize movement policies, update
sector readiness/collision, commit ready transfers, move ready players, enforce
surface constraints and reset jump intent. Module dependencies cannot interleave
inside that transaction. Startup remains once per prepared world after restore;
replication remains independently scheduled outside the fixed tick. The module
registry rejects unknown/reserved/duplicate/cyclic dependencies and reverse
phase constraints before runtime startup. See the gameplay README for stable
built-in IDs and extension boundaries. Graph allocation is startup-only;
dispatch reuses the immutable plan. No tick cadence, monotonic clock, catch-up,
protocol, save or generator changes.

`ServerRuntime` owns mutable authoritative state on one lane. Worker threads do not mutate active worlds. Signal, admin `server stop`, and host-owner shutdown converge on the shared `ShutdownRequest`. Shutdown closes game and admin admission, rejects pending admin commands, then keeps the owner lane frozen while bounded workers stop and the final snapshot is captured and persisted. A single absolute deadline is 30 seconds from lifecycle start; worker, save, endpoint/status work has the first 28 seconds and the host diagnostics flush uses the remaining two seconds. A timeout is reported as failure and does not mean blocking operating-system I/O was cancelled. `Attached` client sessions never stop the server; only an `OwnedServer` sends graceful stop and reaps its child.

`GameState` owns actor-keyed fire and per-player chat cooldowns, plus the global
chat cooldown. The closed `ActorAction` admission operation derives its own
`WorldInput`, checks cooldowns before queue capacity, and records cooldown time
only after inserting into the bounded owner-lane queue. Admission supplies the
transport-capacity result and retains permission/context checks. Action state is
created only after membership reservation and removed with the actor on Leave;
transfers retain the same actor cooldown state. Generic queue admission rejects
Fire and Chat so these actions cannot bypass their policy.

`runtime/operations.rs` keeps the loop order explicit and delegates autosave scheduling/capture to `poll_autosave`. `lifecycle.rs` coordinates admission closure, bounded admin drain, client socket closure, worker stop, final snapshot capture, persistence shutdown, endpoint cleanup and shutdown status in that order. The shared request carries the absolute diagnostics deadline back to the process host. Storage owns snapshot serialization and worker status; the runtime supplies the live world data and final snapshot. Persistence timeout/error and unfinished worker IDs propagate to the process result; the last atomically replaced snapshot remains intact.

`WorldRegistry` is the stable runtime façade for world instances. Its `membership.rs` module owns membership and epoch changes, `transfers.rs` owns staged and committed transfers, and `accessors.rs` owns read-only queries and save snapshots. Constructors and entity, sector, environment, and diagnostic operations stay in their dedicated modules.

Gameplay systems receive the owner-lane simulation tick, read-only component/event views, and a per-world New/Restored lifecycle during Startup. Startup runs once after each world is restored and before initial entities are marked initialized; saved initialization markers prevent portal reseeding. Systems return bounded effects and the runtime produces an aligned, bounded outcome for each retained effect. Entity mutations form one atomic subgroup: validation or storage failure rejects each entity member without publishing any member, while success publishes stores, replication journals and snapshot budgets together. Transfers are processed independently in their original order. Each reports Started only after the pending prepare message is queued, or a typed rejection for validation, an existing pending transfer, exhausted identifiers, missing client, unavailable environment, or backpressure. Any pending transfer created before a later client, environment or output failure is cancelled. Started is not transition readiness or world commit. Mixed entity and transfer outcomes are partial by design and do not claim transport/entity atomicity. Buffer-rejected pushes remain a saturating aggregate and reject the retained stage effects as a whole.

`sector_streaming.rs` coordinates the per-tick pipeline. `sector_streaming/requests.rs` collects read-only player and transfer requests; `tickets.rs` owns retain/pin/release reconciliation; `workers.rs` dispatches generation to a fixed worker pool with a 64-job bound and installs up to eight completed results per tick; `readiness.rs` evaluates collision-ready players and transfers; and `collision.rs` rebuilds changed instances from installed payloads. Worker count defaults to `min(max(available_parallelism - 1, 1), 4)` and `--sector-workers` or `server.sector_workers` accepts 1..16 or `auto`.

Combat readiness computes closed ray/sector intervals after normalizing the ray
with the simulation's f32 hitscan rule. It rejects rays exceeding 512 m, more
than 4,096 intersected sectors, or more than 65,536 conservative candidate
cells before sector iteration. Those capacity failures return Busy without
applying the shot; the next attempt can request a new bounded demand.

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
it owns four bounded I/O workers and a private validated file snapshot, and joins
the workers before snapshot cleanup at shutdown. Open the selected HTTP
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
Local-water capacity defaults to 131,072 cells per world and 524,288 per server.
Use `--fluid-world-cells` / `--fluid-server-cells`, canonical settings
`server.fluid.world_cells` / `server.fluid.server_cells`, or environment variables
`MINIMIUM_SERVER_FLUID_WORLD_CELLS` / `MINIMIUM_SERVER_FLUID_SERVER_CELLS`.
World capacity accepts 1..262,144 and must not exceed server capacity; server
capacity accepts 1..524,288.
Tool results retain one response slot per player and retry admission to the
bounded output queue before terrain data, so output pressure cannot strand the
client's pending tool action.

`server.autosave_interval` is the first live server property. Its change runs on
the owner lane, updates the autosave deadline, and leaves a save already in
progress untouched. Clock properties use the existing `TimeCommand` and
`EnvironmentClock` validation; world seed and generation identity reads come
from the active manifest.

Streamed sector geometry is captured as an immutable per-world snapshot and a
revisioned collision index is built by a fixed worker. Pending snapshots for a
world coalesce to the newest revision; stale results are discarded. Movement
and transfer readiness wait until the matching complete index is installed.

The worker publishes movement collision and exact-sector combat replacements
together with the full-map combat baseline in one `SpatialGeometryPublication`.
Excavation swaps the same bundle only after durable acknowledgement; restore
rebuilds current sector payloads through ordinary bounded streaming. A hitscan
computes a conservative set of sectors across its full configured ray range.
Missing sectors create one bounded demand per actor/context, pinned through the
existing sector manager and worker pool. Combat demand has separate tickets and
readiness from movement; demand is released on a successful later shot, actor
removal or context change. Capacity failure returns `Busy` with diagnostics,
and the original shot is not replayed.

Authoritative hitscan demand and simulation both derive the ray origin from the
world `CharacterGeometry` eye point. Excavation tool admission uses the same eye
point and body frame as client tool previews.

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

## Authoritative meteorites

`spawn_random_meteorid` uses the existing local admin/owned-server command path.
It accepts one event, with a five-second cooldown and a global eight-flight cap.
Placement makes up to 32 land attempts near an active planet player, biases manual
events to recent gaze and checks players, spatial entities, boxes, shore and slope.
Natural events roll the sample profile once per existing 120..300-second opportunity.
The private impact owner holds live `BodyState` values; the entity component is
only the last durable checkpoint projection. The owner lane integrates
`BodyState`, reserves entity/snapshot capacity, queues an immutable journal
transaction and publishes the projection plus terrain only after its durable
acknowledgement. Parent and daughter share one commit. Acknowledgements update
the projection only when they match the owner's exact pending checkpoint; a
stale or duplicate acknowledgement requests shutdown before changing the
projection or pending state. A matching acknowledgement does not rewind a
newer live state. Generic entity effects
cannot spawn, replace or remove the impact component, or despawn a protected
live or checkpointed impact entity. Restored state is bound to world ID, seed
and radius before startup systems allocate new entities.
The private `ExcavationCoordinator` owns the single preparation/durable-write
slot, mutation IDs, journal acknowledgements, failure and shutdown state. Tool,
impact, inventory and fluid-checkpoint producers use its controlled operations;
runtime methods cannot replace its pending state. Its work-submission port uses
the bounded shared pool and owns impact checkpoint reservation rollback. Its
publication port receives only the required world/sector/impact services and
ancillary excavation data, never `ServerRuntime`. A private-constructor
`DurablePublication` proof is created only after acknowledgement identity and
impact checkpoint checks; only that proof can invoke live publication.

Stale context/content versions and failed worker results never reach the journal.
Queue acceptance is distinct from durable acknowledgement. Shutdown drops the
unsubmitted result receiver and releases its exact impact checkpoint reservations;
accepted writes still drain under the existing deadline. Inventory, hydrology
cadence and network cursors remain separate from transaction state. The current
file journal backend and host shutdown polling remain adapters pending S12.

The shared excavation coordinator prepares replacement geometry, collision leaves
and displaced water before durable commit. Acknowledgement installs the prepared
parts together and invalidates intersecting nonresident tickets. Movement and
transfer readiness require the matching installed collision tickets. Unaffected
sectors retain their indexes. The status command reports active/pending bodies, crater
count and storage health. Ordinary impact I/O failure stops new impacts but preserves
replication of the last committed world. The original automatic scheduler remains transient.

Current terrain preparation uses `TerrainInputs` with separate geography,
resolved material profile, resolved relief policy, crater and remnant snapshots.
`WorldDefinition` supplies the relief policy selected by content composition;
the registry binds and checks it against the exact manifest and map fingerprint
before publication. Generated planets reject missing policy, and duplicate
content identities cannot select conflicting material or relief profiles.
The sample composition supplies its canonical relief policy and 1.5 m material
profile, while the runtime has no independent soil-depth override. Entity batches clone only affected world stores, while
capacity checks still include all untouched entities. Impact settings enter via
`ServerComposition::impact_profile`, without a specialized module hook.
`runtime/excavation` coordinates tool intents, profile inventories, immutable
terrain/collision preparation, fluid preflight and coherent checkpoints. The
shared worker pool runs preparation and water steps; only the owner installs
acknowledged changes. New impacts use this result journal for crater, remnant
and water publication. Bounded contextual chunk/fluid cursors share a structural
batch; fluid and terrain terminal markers are emitted once, with BatchEnd held
until its paired fluid snapshot completes. Inventory and action replies remain
small messages. See
[excavation](../../docs/excavation.md) for limits and deferred acceptance.
Application composition supplies the validated per-tool profile. The owner
selects its definition from the confirmed inventory, performs the first-hit
query with that definition's reach, and passes a validated radius/material set
to `minimium-terrain`; the durable-before-publication flow is unchanged.
Unsafe geometry rejections retain their contour failure category in a bounded
diagnostic with mutation/action IDs, brush center and at most eight chunk keys.
