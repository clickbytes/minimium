# minimium-client

`start-earth.ps1` starts its owned local server in developer mode, granting the
local profile the `DeveloperTeleport` permission required by `map_telepo`.
The desktop quit path hides the window before bounded worker and diagnostics
shutdown, so a final save does not leave a frozen frame on screen.

## Purpose / Non-goals

Owns the desktop and text client application shells, input, console, renderer and command presentation. It consumes session, connection and clock synchronization contracts from `minimium-client-runtime`.

Console input, terminal input, and bounded developer stdin use one typed text
command parser/executor. The executor returns local queue outcomes and typed
action or presentation results; each host formats those results itself. Developer
stdin does not edit console text, history, or preedit state. Renderer-only
diagnostics are explicit presentation operations and return a host-unavailable
result in the terminal client.

## Public API

Cloud debug properties use the renderer's CPU `cloud_presentation::CloudDebug`
catalog. The bounded cloud mesh cache asks `CloudTessellation` for the observer-
and settings-selected sampling grid; it retains worker cancellation, sample/face
budgets and publication ownership. Domain weather supplies physical clouds only.

Executable entry point is `src/main.rs`; internal modules provide startup and front-end behavior.

Terrain and weather workers propagate geographic sampling errors to session
diagnostics. Sector requests publish only complete prepared scenes; a failed
request retains the previous complete publication. Weather atlas, cover and cloud
results each publish only after successful preparation and matching context.
CPU preparation errors neither upload partial geometry nor synthesize heights.

Cloud worker candidates replace the active CPU publication only after the whole
cloud GPU upload is accepted. Cloud/morph count mismatches and upload-limit/index
errors produce diagnostics while keeping the previous complete publication.
Context changes still invalidate old clouds; stale worker results are discarded.

Remote-player GPU geometry carries the exact active world context (including
membership epoch) and full geometry publication stamp. Missing or mismatched
contexts/publications clear its selection; scene encoding checks the stamp again.
An origin or publication change rebuilds even stationary player geometry, and
disconnect/reset invalidates the stamp together with the buffer. Waiting scenes
cannot draw players retained from a previous world.

The desktop host reads the confirmed equipped tool from the active client
session, supplies its selected `ToolDefinition` to the tool preview, and resolves
its sample-owned model index for the first-person viewmodel pass. The application
adapts the sample's CPU vertices to the renderer's bounded neutral model catalog;
the renderer owns upload and draw only. Shovels and pickaxes are presentation-only;
gameplay tool use continues through the existing authoritative session action.

Camera and tool-preview rays use the shared world `CharacterGeometry` eye and
radial frame. Remote body meshes and shadow casters are generated from its
0.6×1.7×0.6 m local body box, oriented by the same frame used by simulation
hitscan. FixedY and radial characters at +Y use right +Z, up +Y, and forward
+X; because both horizontal body dimensions are 0.6 m, their spatial boxes
match the previous axis-aligned bounds. Radial characters rotate with their
local up direction elsewhere.

## Dependencies

The default `desktop` feature enables renderer GPU code and the astronomy,
window and clipboard adapters. `cargo build -p minimium-client --no-default-features`
builds a terminal-only executable: it defaults to console mode, needs no material
package and has no wgpu/winit/astronomy dependency. Renderer quality/debug values
remain CPU-only public contracts when its `gpu` feature is disabled.

The renderer's `RenderFrame`, `PreparedSector`, uniform and vertex data are also
available without GPU. Desktop exposure calculation and diagnostics share
`render_frame::view_exposure` with CPU consumers; upload still uses the same
renderer data types and feature-gated GPU owners.

Consumes renderer, client-runtime, core, environment, world, assets, asset-io, astronomy, platform, protocol, procgen, sectors, telemetry and sample-game content. The application supplies the client-runtime world-content resolver shared by desktop and terminal frontends. Headless server and ctl must not depend on this package.

## Data flow

Client preferences resolve `ui.locale` to `en` (default) or `pl`; `--set
ui.locale=pl` and the client configuration file select the same typed locale.
The `minimium-localization` formatter owns Fluent catalogs and per-message
English fallback. Both frontends use typed localized messages for session
events and connection/window status. `set ui.locale en|pl` changes the active
locale without reconnecting; `reset ui.locale` restores the startup setting,
and `config save` persists an active override. Property help, property lists,
command help and autocomplete guidance are localized at display time. The
desktop console retains typed messages so changing locale updates their
displayed history.

The console UI rasterizes bundled Roboto Mono with `fontdue` at a fixed 20
logical-pixel size into a 32-pixel-cell CPU atlas bounded to 1 MiB for Latin,
extended Latin, combining marks, punctuation and a replacement glyph. Atlas
packing keeps complete measured glyph bounds; oversized glyphs fail the atlas
invariant instead of being silently clipped. Text placement uses font advance,
bearing and ascent metrics. UI geometry uses logical window coordinates and the
physical framebuffer scales it by the current DPI factor; raster size is fixed
across DPI changes. Input navigation and deletion use grapheme-cluster
boundaries from `unicode-segmentation`; empty IME preedit ends composition and nonempty preedit remains a
separate composition state and only Commit enters the command line. RTL and CJK
input are outside this scope.
The desktop input mapper clears held controls on focus loss and when the console
or map changes the active input context, including map changes initiated by a
server message. Its action dispatch suppresses gameplay actions outside gameplay,
map toggles while console or modal input owns the event, and console toggles
while modal input owns it.

Front ends resolve profile, server-data, material-cache, config and log paths once through `minimium-platform::AppPaths`, then pass the selected absolute paths to client runtime and locally owned server launches. `--log-dir` selects the process log root and defaults to `./logs`. One application-owned world resolver is shared by desktop and terminal; it verifies generated or authored content against each server manifest before runtime and presentation consume the map. Both hosts call `ClientApplication::step` with monotonic time; it polls and applies CPU session state and advances fixed-step prediction when the desktop supplies gameplay input. The desktop event-loop deadline runs independently of redraw and `RedrawRequested` only prepares and submits the latest view. Terminal calls the same step with neutral prediction input and no GPU construction.

## Ownership and execution

`ClientBootstrap` composes both hosts without GPU dependencies. It resolves the
initial map/instance and catalogs, process paths, profile, diagnostics loggers,
replica schemas, authentication, cache limits and sample content resolver, then
returns one configured idle controller. The prepared connection is consumed with
explicit CPU-only or desktop capabilities; desktop compression support comes
from device features/limits. Bootstrap does not connect or launch a server.
The hosts retain their diagnostics lifetime and 30-second shutdown deadline.
Authored instance 1, manual dungeon instance 4 and the sample primary instance
are selected by the common composition, without a fingerprint-based lookup.
The former terminal-only world setup and duplicated desktop setup are removed.

Before a session exists, desktop initialization and application scene requests
bind the local map/manifest to a `PreparedTerrainSource` through
`world_content::bootstrap_terrain_inputs` and pair it with an empty, instance-bound
`TerrainOverlaySnapshot`. `TerrainInputs` has no default or partial-identity path.
`ClientApplication::scene_terrain_inputs` returns no source while bootstrap Earth
geography or context-matched session content is still pending; this leaves scene
and prediction readiness pending without publishing empty geometry. Once connected,
the session supplies the active source for the active context and the pending
source for its matching transfer ticket. Distinct destination content does not
borrow active-world overlays; when both contexts refer to the same instance,
manifest and map fingerprint, the runtime intentionally shares that world's state.
Bootstrap does not open Earth geography; the content
resolver prepares it and validates its digest before Earth geometry is dispatched.
Protocol, save, seed/fingerprint and material-package formats are unchanged.

`DesktopHost` and the terminal front end call `ClientApplication::step`; the
application owns desktop UI state and polls the runtime connection controller.
The runtime owns bounded logical protocol state and the local-server guard.
Its connection controller supplies the shared TCP adapter from
`minimium-transport`; background connector work reports typed results.

The desktop host gates steps on a monotonic 16 ms deadline in `about_to_wait`,
including after input events, so missed deadlines do not accelerate the cadence.
Session polling and fixed-step progression do not depend on a redraw callback;
minimized and occluded windows continue the application loop. The step caps
elapsed prediction time at the existing simulation step, applies prediction
before polling session messages, and returns typed connector/session updates.
Desktop candidate movement is retained only after the host has requested and
prepared the candidate sector scene, the application has accepted its
context/manifest/terrain-tagged collision report, and planetary water covers
the candidate capsule. `sectors_ready` continues to describe the currently
covered region; a blocked candidate keeps its interest for the next scheduled
step and cannot queue motion, while the covered region remains available to
retry the same boundary crossing.
The terminal calls the same step before its existing bounded 25 ms input wait,
with prediction omitted so it cannot enqueue synthetic movement. The redraw
callback prepares camera/environment presentation from the latest immutable
application view and submits it without polling or stepping a session.
`DesktopHost` also owns one monotonic cosmetic presentation epoch initialized
with the host. Each prepared frame receives its elapsed `Duration`; redraws,
resizes and world transfers do not reset it. This clock does not derive from
world UTC or affect session polling, prediction or simulation.

Both frontends display the controller's typed busy rejection. Desktop switches
its scene to connecting only after an attempt is accepted; a rejected restart
keeps the active environment and geometry visible.

`ClientApplication` is the shared CPU state owner for desktop and terminal
connection, session updates, actions, fixed-step accumulation/prediction and the
active world catalog. Its step applies CPU message results once and returns
world-context changes for host-only presentation installation and cleanup.
The desktop `DesktopHost` owns it alongside a GPU-only `Renderer`; the renderer
has no client session or gameplay state. The host builds immutable
`DesktopRenderView` values for frame preparation and reports presentation
readiness with the transfer ticket and terrain revision used to prepare it.
The application validates that report against the current pending transfer and
CPU content before asking the runtime to queue readiness. Terminal mode uses the
same CPU content gate and retries until its resolved map, manifest, planetary
geography and planetary water catalog are available. GPU readiness never
authorizes a world change.

The client runtime owns transfer ticket lifecycle and protocol readiness. A
pending transfer retains its resolved map behind its context and transition ID;
preparing destination geometry does not overwrite the active world catalog.
The application installs that map in the active catalog only after commit.
Cancel and failure drop the staged map. `DesktopHost` owns gameplay render
geometry preparation and hit feedback; `Renderer` owns GPU resources and frame
submission. The application action executor owns the mouse weapon-fallback
cooldown and reports the resolved queued action kind; the application has no
GPU handle.
The transition and overview fields are private to `ClientApplication`. Desktop
host consumers read the overview model through an immutable view and request open,
close, zoom and drag transitions through application methods. Open state is the
presence of one world instance, so an open map cannot lack an instance. The
overview model owns finite, bounded zoom and transient drag state.
`DesktopHost` owns CPU shadow selection state and passes it to GPU submission
through the immutable per-frame scene snapshot; `Renderer` owns caster buffers
and uniforms, not shadow validity or source selection.
The snapshot carries the renderer's complete geometry publication stamp and keys;
scene receivers, selected casters, remote casters and the uploaded map must match
the same world, content, publication and floating-origin revisions before shadow
encoding is enabled.
The active scene revision and origin generation in sky diagnostics come from
the current geometry publication; separate expected fields show the prepared
shadow's stamp when it is stale. The incomplete reason distinguishes missing
source, budget failure, stale context and pending/failed upload.
`shadow_prepare::prepare_shadow_frame` performs CPU-only selection, bounds checks,
uniform preparation and bounded local-map geometry conversion. `Renderer`
owns shadow buffer allocation/upload and pass encoding; it publishes cache
metadata only after the complete upload. `DesktopHost::release_world_sectors` is
the single caller-facing world release point, clearing CPU map preparation and
GPU world/shadow buffers together. The shadow CPU timer includes preparation
and reads of immutable publication metadata, but no buffer upload or encode.
`SettingsStore` is the single owner of resolved startup requests, session
overrides and their property sources. Renderer/GPU state holds values needed
for current presentation; `reset` returns to the resolved startup layer and
`config save` persists store overrides through the configuration owner.
Desktop and terminal clients share the command registry/handlers and
`client-runtime` session and replica contracts.

Client-authored world actions pass through one application-owned typed executor
shared by desktop input, console commands and the terminal. It validates the
current session, constructs protocol requests and reports local queue acceptance,
suppression or rejection; queued requests are not authoritative outcomes. The
`UseEquipped` input intent resolves confirmed tool inventory in that executor:
pressed input may fall back to weapon fire, while held repeats only repeat an
equipped tool. Explicit `combat fire` always requests weapon fire. Renderer code
supplies CPU movement and aim parameters but does not choose the action rule.

Confirmed developer tool inventory enables the desktop digging overlay. It uses
the shared simulation first-hit query against published collision and remote-player
capsules, then samples current terrain material through the profile bound to the
resolved manifest and map, matching server material classification.
A green aim/brush outline means local material eligibility; red marks a blocking
object or unsuitable material, and amber indicates missing range/readiness or a
pending action. The server remains authoritative. The last action result remains
visible until another result or context change, distinguishing obstruction from
unsafe geometry joining. The overlay and console text support English and Polish.

Both frontends install the same `SampleWorldContentResolver`. It validates the
revision-6 manifest and expected map fingerprint, loads only configured Earth
geography for Earth manifests, and recreates generated maps from topology and
the recorded seed. It returns a private `PreparedTerrainSource` inside
`ResolvedWorldContent`, bound to its `ContentResolutionTicket`, instance,
manifest, map fingerprint and resolved sample profiles. Authored content is
accepted only from a known or transferred map whose fingerprint matches; missing
authored content is an error. Generation failures retain their `GenerationError`
source, stable code and content ticket in `SessionFailure` returned to either
frontend.

Desktop and terminal register sample-game component schemas with the runtime
replica store. Portal commands resolve the nearest portal from the current
world-scoped replica view and send its entity ID; the server remains
authoritative for interaction range and transfer validation.

Desktop and terminal shutdown each establish one absolute 30-second deadline.
Connection, owned server and renderer workers use its first 28 seconds; the
diagnostics writer receives the remaining time.

## Invariants and errors

Preserve desktop/text command behavior and keep protocol/session handling out of rendering algorithms.
Ambient meteors (E047) stay cosmetic and client-local: one client monotonic clock
seeds the deterministic manual events, the bounded meteor contract dismisses every
pending event instantly on world/teleport/reconnect/interior/unavailable changes,
and the planet scale always resolves from the world manifest.

## Commands

The shared command registry owns argument metadata (type, optional/rest behavior,
choices, constraints, unit and description). Console help and enum completion
derive their output from those descriptors; command execution still delegates
validation and state changes to the owning domain handler.

Terrain status reports shared overview/FPP preparation and geometry residency.
Terrain debug habitat/source inspects landform influence and coverage provenance.
Weather status reports worker history readiness and material memory.
Both planet cameras select/render the same surface; presentation interest is
separate from player collision interest. `start-earth.ps1 -ResetWorld` is now an explicit switch: it keeps
the old world in a sibling backup and refuses an active server's file lock.

Atmosphere presentation uses the actual render camera and its world/sea-level
scale. Render-only astronomical observers follow the camera's latitude/longitude
with a capped reference height within the astronomy contract; optical rendering
uses the uncapped height. Player/domain observer contracts remain unchanged.
Regional weather/cover/fog requests follow the view, with UTC-hour endpoint
interpolation. Workers supply sea-level-relative ground and a bounded fog grid.

`sky meteors status|spawn faint|bright|fireball|clear` run the client-local
ambient meteor runtime: each desktop command spawns one bounded, deterministic
diagnostic event that initializes only on valid environment frames, rebuilds the
CPU `MeteorFrame` once per frame and never touches server, protocol, save or GPU
state (visible sky integration is E047-03, the automatic generator E047-04).

`atmosphere status` reports quality, optical altitude, logical GPU memory and
optional asynchronous GPU timings. `atmosphere debug
off|transmission|scattering|optical_depth|distance|fog` selects atmosphere diagnostics.
Atmosphere and terrain diagnostic selections are mutually exclusive; diagnostic
views suppress cloud overlays. Terminal builds parse these commands but require
the graphical frontend to display them. No save reset or protocol change is needed.

The application folds all four words of the 128-bit generation seed into the terrain
uniform for deterministic local habitat variation. It updates the UTC interpolation
phase of the two worker-supplied cover endpoints. This does not change world
generation revisions or saves, and needs no world reset.

`terrain debug off|albedo|normals|depth|shadow|base_color|coverage|habitat|source` selects terrain diagnostics in the
desktop console. The text frontend reports presentation-only commands without
constructing GPU resources. Desktop startup loads the cooked offline base set;
server-required sets are installed before content readiness. Distribute the
executable with `materials/manifest.ron` and its declared sibling maps using
`tools/package_client.py --manifest PATH EXECUTABLE NEW_OUTPUT_DIRECTORY`.
Cargo does not cook materials. Development launches explicitly select a cooked
manifest through `MINIMIUM_MATERIAL_MANIFEST`; otherwise the desktop loads the
installed manifest beside its executable. Both startup and session material
preparation use that same selection and bounded validation. An invalid explicit
path fails without selecting another recipe. `start-earth.ps1` runs the sample
cook step explicitly, then passes and restores the manifest environment variable.
`albedo` retains shaded uniform grey; `base_color` shows actual material color
without lighting, and `coverage` shows a fixed six-role palette without textures.

Client command groups declare specs in `src/client_commands/`; matching and alias resolution use `minimium-commands::CommandRegistry`. Client action execution remains here.

The client property catalog is registered beside those command groups. It owns live client presentation preferences, local movement input, and the replicated world-time view. Legacy camera, movement, and time commands share property validation and owner handlers. The desktop renderer reports requested/effective values and GPU fallback; the text frontend reports the same requested value and source with effective state marked unavailable when it has no renderer. Help, list, and completion read these registered descriptors, including ranges and enum values.

Client preferences use schema v1 RON and resolve default, file, mapped environment, then CLI input. `config validate` and `config show` run before renderer startup; `config show --sources` reads the same requested/source record and reports effective as unavailable with a configuration-preview reason because no client consumer has applied the value. Live property reads use that record after application and renderer results update effective/reason. `config save PATH` writes only persistent requested values and uses an atomic replacement. Session values reset to the resolved startup layer and are never written to the source file automatically.

The canonical property keys and descriptors also define the complete local
startup catalog, defaults and persistence flags. `StartupSettings` retains the
validated `ClientPropertyValue` values and their sources; desktop, terminal,
configuration preview and `SettingsStore` consume that same immutable set.
Values encode through the property contract, never Rust `Debug` names. Reset
restores the typed startup value and source, then reapplies the host operation
and its effective-value constraint. This includes `optical_depth`, `base_color`
and `client.audio.meteor_volume` (default 0.25, session/CLI only). The historical
`client.audio.meteors` command alias maps to the canonical sound key without
creating another startup record. No protocol, save or content format changes.

C01 runtime acceptance is deferred: check every local property's set/reset,
both diagnostic modes across reset, a non-default CLI meteor volume followed by
set/reset, source precedence and persistence, and terminal unavailable reasons.
Static compilation covers both host feature configurations; it does not prove
visual or audio behavior.

Local server startup passes a typed launch configuration to client runtime. The runtime asks the sibling server executable for offline `config show --sources --format json`, validates local bind/headless constraints, and uses the same captured environment and argv settings for launch. Explicit startup settings prevent attach to an existing server; `--server-data-dir` remains a target selector.

## Change guide

`ClientApplication` in `src/application.rs` is the private mutable owner of logical game/session state, settings, world catalog, UI, input mapping and one bounded staged transfer record. Desktop code gets immutable game/catalog views; actions, prediction steps and rollback, collision readiness, world-context changes, health and lifecycle resets go through owner operations. A world-context change returns the previous and new identities so the desktop host can release source presentation resources after the application has accepted the server message. Active collision publication carries its `WorldContext` and `SectorContentVersion`; bootstrap publication carries instance, manifest and map fingerprint. The application rejects reports that no longer match its current world/content. Client-runtime owns transfer lifecycle, readiness queue retry, prediction, reconciliation and remote-player snapshots; the application drives its client-runtime operations. `GameplayPresentation` owns renderer-only hit feedback and geometry. The application action executor owns client-authored action validation, request construction, queue outcomes and mouse weapon-fallback cooldown; its typed intent path is shared with console and terminal command dispatch. The application owns the shared generated/authored world resolver used by desktop and terminal. `DesktopHost` owns `ClientApplication` and the GPU-only `Renderer` as peers; `RenderFrame` is CPU-only input to renderer upload. The application has no GPU handle. `renderer/window_events.rs` maps physical events into `InputAction`s and applies modal priority; text and IME input remain separate. `application/transitions.rs` stages destination content, `session/sector_readiness.rs` prepares and publishes desktop scenes before reporting host readiness, and the terminal reports host readiness when CPU content resolution completes.

To add an input action, extend `InputAction::parse` and the default/context binding table in `src/input.rs`, then route its behavior in `renderer/window_events.rs` or the relevant application handler. Keep binding validation and persistence through `SettingsStore`. Desktop and terminal share configuration; terminal has no renderer event loop. To add a UI sentence, add a typed `UiMessage`/`UiNotice`, English and Polish Fluent entries, and construct the message at its consumer; `Localizer` handles fallback and both frontends format it.

## Verification

Run `rtk cargo check -p minimium-client` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.

`rtk proxy python scripts/check_feature_contracts.py` independently checks and
compiles the terminal feature selection without GPU/window dependencies.


Terrain presentation uses a latest-request worker per interested world; immutable
geometry and prepared vertices are cached outside the render loop. The renderer
limits new uploads across the entire frame, preserving the previous active scene
until its replacement is complete. Collision readiness remains separate from
presentation readiness. Portal targets prepare independently of the active world.
The floating origin has 4 km hysteresis; compute-capable rendering rebases
resident vertices instead of discarding all sector buffers.

`fly on`, then `set_altitude 10` places the player's feet 10 physical kilometres
above the local terrain (water surface over oceans), at the same latitude/longitude.
Fractions such as `set_altitude 0.5` are accepted. The range is 0..10000 km,
also subject to world-coordinate limits. Earth height compression is respected.
The authoritative server checks fly mode and current membership, resets velocity
and preserves fly settings. The terminal frontend supports the same command.
The active wire contract is protocol v20 on both client and server. Existing saves use the separate v10 storage contract.

Set `MINIMIUM_FORCE_HDR_FALLBACK=1` before starting the desktop client to compare
the sky and bolid brightness with the non-HDR output path on an HDR-capable GPU.

Planet collision materialization uses a separate bounded worker (32 total queued,
running and completed jobs); SectorManager installs results on its owner lane.
World admission can await collision/presentation asynchronously without treating
a pending mesh as an unsupported world. Walking waits for collision readiness.

Renderer bootstrap accepts pending collision sectors and partial GPU uploads.
It starts with an empty collision index, walking disabled and terrain hidden,
then polls the existing frame readiness path. Earth jobs wait for validated
session geography before dispatch; pending work is not a graphics startup error.

Desktop starts in borderless fullscreen at the monitor resolution. F11 toggles
fullscreen/windowed mode, including while the console is open. --windowed (or
start-earth.ps1 -Windowed) starts windowed at screen position (0, 0), with the
outer window sized to the primary monitor, accounting for title bar and borders.
Runtime window/DPI acceptance remains pending.

The terrain worker prepares paired Earth ground/ocean patches. Both are staged
as one selection, including during transfers and render-origin changes.
The worker now clips complementary shoreline polygons before preparing vertices;
nearly coplanar land/water depth comparisons no longer choose the contour. LOD
requests preserve camera radius after direction snapping, and walking detail has
a bounded local guard. These changes use the existing terrain worker and caches.
The terrain status command reports ocean triangles and includes ocean buffers
in resident surface memory. Existing Earth worlds use this presentation on the
next normal start; ResetWorld is unnecessary. Visual acceptance remains pending.

Earth launcher peak settings are opt-in for new worlds:
- PeakMultiplier: maximum 1..3, default 1.5 when any Peak option is supplied.
- PeakExponent: integer 1..8, default 3; larger values concentrate gain near peaks.
- PeakStartMeters / PeakFullMeters: source-height thresholds, defaults 1000 / 9000.
Ordinary starts restore the saved immutable definition. Peak options with an
existing world require a new DataDir or explicit ResetWorld, which keeps a backup.

ClimateReliefInfluence is independent: 0 keeps source-height climate, 1 follows
the peak boost, and intermediate values blend them. An explicit value is persisted
by the server at startup; changing this option needs no world reset. Omission
restores the saved climate setting, with 0 as the legacy/new default.
Both client/server must be restarted for protocol v19. Terrain status reports
curve thresholds/exponent/gain and the active climate influence.

The Earth launcher explicitly removes empty/omitted process environment options
and restores prior nonempty values in its finally block. Bootstrap additionally
ignores empty entries, preserving saved configuration on ordinary resumes.

Overview supplies its camera position, FOV and viewport to TerrainView::overview
for guarded frustum selection and a bounded 512-leaf terrain budget. FPP remains
at 256 leaves. Terrain status reports the requested view's budget; worker and GPU
upload scheduling are unchanged. Coastline and frame-time acceptance are pending.

Planetary overview now participates in the same single-source shadow pass as FPP.
The client supplies camera/active bounds, chooses the dominant viewed Sun/Moon,
and selects complete active terrain casters around the renderer-fitted region.
Authored-map casters use a padded spatial cache instead of rebuilding for every
sub-metre movement. The existing 2 MiB map-buffer bound remains enforced.
Overview does not depend on remote-player caster readiness. sky status reports
the actual shadow_range_m and shadow_texel_m, alongside source/completeness.
Existing worlds need no reset. Lighting and performance acceptance remain pending.

The client builds a complete collision index from the current immutable sector
interest on a worker. Scene readiness remains pending until that interest's
revision is installed; an obsolete worker result cannot replace a newer one.
If admitting a replacement interest fails partway through its sector requests,
the client unpins and cancels every ticket already admitted for that interest.
Planet and dungeon sector materialization share the bounded collision worker;
terrain preparation replaces pending work with the latest world/manifest/view/
origin request and publishes only a full-key match.

Desktop and terminal bootstrap explicitly select `dev.profile` credentials
derived from the locally stored profile ID. This is a local identity claim for
development; it does not provide accounts or secure public authentication.

Input bindings use the `client.input.bindings` configuration setting with
semicolon-separated `context.action=control` entries. Contexts are `global`,
`gameplay`, `console`, `map`, and `modal`; actions include movement, jump,
interaction, fire, console, map, and fullscreen. Defaults preserve the current
keyboard and mouse controls. Bindings are validated while loading configuration
and can be changed/reset with the local property command; the desktop replaces
the active mapper atomically, and the text frontend can save an override for a
later desktop launch. Physical controls remain separate from text and IME input.
`input::PhysicalControl` is the shared configuration/device catalog: named
keyboard controls retain their existing names, and mouse controls are
`MouseLeft`, `MouseRight`, `MouseMiddle`, `MouseBack`, and `MouseForward`.
Unknown names are rejected before replacing the mapper. Both devices use the
same typed press/release and action dispatch; for example,
`gameplay.fire=MouseRight` keeps the existing click-to-capture behavior. Left
click selection in the console and map dragging take priority over bindings.

The F1 console displays a bordered overlay with scrollable command output;
PageUp/PageDown and the mouse wheel scroll history. `help` lists short command
usages, while `help COMMAND` and `help KEY` show detailed command and property
descriptions. Invalid command arguments display the expected usage. The game HUD
is hidden while the console is open.

For local desktop diagnosis, `--dev --dev-stdin` (or `start-earth.ps1 -DevStdin`)
reads bounded commands from standard input after the scene is ready and prints
their console output to standard output. It is unavailable with `--connect` or
the terminal client. `dev goto LATITUDE LONGITUDE` requests the existing
server-authorized Earth map teleport; the next input waits for its result.
`dev look YAW_DEG PITCH_DEG` sets only the local diagnostic camera direction.
`dev clouds` lists up to eight mature, above-horizon cloud targets from the
published weather snapshot, ranked by optical depth times angular extent. Each
line includes a `dev look` direction toward the cloud's bounding-box center.
These are aiming hints for manual occlusion checks, not measured line-of-sight
opacity: the voxel mesh may contain gaps. The command does not change weather.
`dev bolid transmission on|off` replaces the active bolid composite with its
foreground-cloud transmission: black means opaque and white unobstructed.
Only pixels covered by a retained luminous/dust layer have a cloud mask;
white elsewhere does not establish clear weather. HDR output still tone maps
the diagnostic, so screenshot RGB is not a numerical transmission readback.
It defaults off and resets when the layer is recreated (including resize).

Bolid appearance is prepared once per frame from server time and the v19 seed,
then shared by sky rendering, point lighting and shadow selection. World-context
filtering and the eight-event bound remain in the client adapter; the renderer
owns deterministic visual ablation and fragment evolution. Manual
`spawn_random_bolid` retains its cooldown and selects a bright example.
Each retained event caches its mass/fragment emission history. The retention
window includes natural emission plus up to 90 seconds of persistent train;
receiving another event no longer discards all trails older than ten seconds.
`dev bolid status` (diagnostic stdin only) reports each cached family, mass
accounting, fracture/grain counts, emission episodes and integration-budget status.

dev bolid replay AGE_SECONDS [SEED] locally replays the most recent current-world
bolid for manual image diagnosis. It reuses its path/kind and optionally replaces
the presentation seed, never sending a new event or changing server state. The
replay age is bounded to 0..120 seconds and requires synchronized server uptime.

Remote shadow batches acknowledge changes to the active geometry publication
even when their vertex content is unchanged or empty. This prevents a stale
publication stamp from disabling the entire shadow map in a single-player
scene or after terrain LOD updates. Origin/context invalidation remains checked.

dev bolid freeze AGE_SECONDS [SEED] uses the same bounded local replay but
holds the selected event at a fixed age for matched sky/light/shadow captures.
dev bolid replay returns it to advancing time. The override is keyed by full
world context and event ID, cleared on scene reset, and never sent to the server.
Normal event expiry and the eight-event eviction bound still apply.

During a bolid point-shadow replay, 	errain debug shadow shows shadow
visibility in grayscale and marks geometrically back-facing terrain facets
in red. This distinguishes receiver acne from genuine mesh self-occlusion;
	errain debug off restores material shading.

dev bolid lighting reports the selected source, incoming camera luminance,
base exposure, night gain and terrain debug mode. It uses the same CPU
extinction and source selection as rendering and does not modify the event.

dev bolid fragments reports the six brightest current emitters per prepared
event, their projected positions and wake lengths in world metres. It reads
the uploaded presentation data and works with fixed-age replay.

Bolid diagnostic replay accepts dev bolid freeze AGE SEED EVENT_ID (and
replay with the same arguments). The optional ID selects a retained event
in the current world; an absent ID is reported, never replaced with the latest
automatic meteor. Without ID the existing latest-event behavior is preserved.
The status command includes peak_radiant_age_s, the first maximum in the
40 ms cached emission samples, to locate a repeatable flash for lighting checks.
This is local developer presentation only; protocol and server events are unchanged.

Release verification: with two retained manual events, an explicit ID replayed
event 1 after event 2 arrived; ID 999999 was rejected. Burst seed 590085 on
event 1 reported peak age 1.12 s. At that age sky info confirmed bolid shadow
source, 25 casters and complete coverage. Capture:
target/bolid-id-selected-peak-shadow.png. This view does not reproduce the
previous grazing-light artifacts and is not their acceptance. Cargo check,
release build, architecture and 500-line checks passed; no automated tests run.

Point-shadow takeover now ramps its occlusion weight continuously from received
luminance 2.5e-5 to 7.5e-5, fades across 140..160 km of world distance and
solar altitude -0.12..-0.18 radians. Eligibility limits are unchanged. This
prevents full map occlusion appearing at the first threshold-crossing frame;
it is a presentation transition, not a physical penumbra or light-power fade.
The weight is carried as f32 bits in the reserved ShadowUniform flags.z word.
Sun/Moon maps ignore that word; direct point irradiance is unchanged.
The local dev bolid lighting command prints the prepared point-shadow weight.
The renderer still owns one shadow map, not simultaneous lunar/bolid shadows.
Release manual replay at Bergen, event 1/Burst seed 590085, confirmed prepared
weights 0.04449 at 1.2 s, 0.54657 at 1.3 s and 1 at 1.4 s (received camera
luminance 3.138e-5, 5.159e-5 and 8.059e-5 respectively). The 4 s extinct
replay returned the shadow source to Moon. These confirm transition plumbing;
they do not establish removal of steep-terrain shadow artifacts or temporal
visual acceptance. Release build, 23 WGSL variants, architecture and 500-line
checks passed; no automated tests were run.

For reproducible visual comparisons across launches, dev bolid status now emits
a replayable line: dev bolid path ID KIND SEED DURATION_MS X0 Y0 Z0 X1 Y1 Z1.
Restore it into a retained event in the current world (spawn a manual event
and replace ID if needed), then select an age with dev bolid freeze/replay.
The path command uses BolidEvent::valid, resets cached evolution and freezes
locally at age zero. It neither injects a server event nor modifies protocol,
saves or the eight-event cap. Reproduce camera/world/time/weather separately.
A low-angle Bergen Burst (manual view yaw 90/pitch 12) again showed triangular
shadow bands at peak age 0.88 s and point-shadow weight 0.9147; capture:
target/bolid-low-angle-shadow-reproduced.png. A higher path at the same terrain
view did not show them. Shared publication keys supply caster LODs; no separate
selection of all cached LOD variants was found. Shadow artifact remains open.
Release replay/restore preserved the same mass accounting, fracture count,
luminous end (1.4 s), peak (4.9519 GW at 0.92 s) and mid-flight NDC. NaN
coordinates and a missing ID were rejected. The restored low-angle path still
reproduces the bands: target/bolid-shadow-fixed-path.png; exact setup and
command saved in target/bolid-shadow-repro.txt. Prepared shadow weight 0.6839.
Compilation, release build, architecture and file-length checks passed.
No automated tests were run.

## Ambient cosmetic meteors (E047-01..03; ADR 0013)

`renderer/meteors` owns local monotonic lifetimes, seeded manual tracks and
world-scope invalidation. `sky meteors spawn faint|bright|fireball|fragmenting`, status and
clear feed a complete bounded `MeteorFrame` into the renderer each frame.
Tracks start at 80..110 physical km and stay above the temporary 20 km cloud
render gate. The seeded velocity has the preset speed; elapsed trail samples
are reconstructed after frame skips and origin changes. Track placement uses
the camera heading without following subsequent camera movement.

`GpuResources::render` uploads only active instances. The shared finite-distance
layer combines cosmetic meteors and existing server bolids, applies terrain,
planet and foreground-cloud occlusion, then resolves before final output/UI.
`sky info` exposes meteor instance/draw counts, upload bytes, low-altitude
rejections and the 9216-byte fixed instance buffer. This is a logical allocation
estimate, not driver VRAM. The automatic generator now schedules bounded
120..300-second monotonic intervals independently of visibility and manual
requests; see `docs/ambient-meteors.md` for seeds, startup controls and limits.
Physical evolution now uses a fixed SI ablation history and energy-based
photometry; no local meteor light is consumed by surfaces yet.
Server-timed `spawn_random_bolid`, protocol, saves and world entities are unchanged.
Static compilation and WGSL validation do not establish visual acceptance;
that remains in the explicitly gated final-verification stage.

Cosmetic fragmentation uses pooled fixed histories with independent solid/grain
tracks and short wakes. Status exposes process events, mass conservation and
omission counters; see docs/ambient-meteors.md. Runtime acceptance is pending.

Residual deposition rings persist after physical head extinction, using bounded
monotonic capture, independent gas/dust TTLs and seeded upper-atmosphere drift.
History pooling now waits for the last train TTL; long afterglows are capped at
two active events. Status reports capture overwrite/skip counters and TTLs.

E047-08 now selects finite-distance meteor visibility by effective sky quality.
Standard uses terrain/planet occlusion plus the shared cloud mask, including
up to 16 containing ellipsoid proxies; low retains the 20-km source floor.
See docs/ambient-meteors.md for resource order, GPU layouts and OIT limitations.

E047-09 uploads a shared 2288-byte, four-source transient-light uniform at frame
binding 11. Physical-distance ranking and shader extinction are producer-neutral;
daylight dust consumes the helper, and sky diagnostics report count/bytes/incident
estimate. See docs/ambient-meteors.md for exact units and limits.

E047-10 connects shared transient lights to terrain/material BRDF, authored
objects and ocean before tone mapping. The session-local command
sky meteors debug light off|on isolates this contribution; source visibility
remains independent. Cloud-path extinction is integrated below; runtime acceptance is pending.

E047-11 adds bounded published-cloud light-path proxies to the same frame
uniform (now 2288 bytes), shared by terrain, world, water, dust and cloud
scattering. The transmission diagnostic and self-shadow approximation are
documented in docs/weather.md; no weather remeshing occurs at a flash.

E047-12 adds a bounded atmospheric halo to existing bright meteor head quads,
with a narrow analytic fallback and no transient LUT invalidation. Shared
emission storage now reserves additive HDR headroom; status reports optical
mode/source counts. Energy and pass-order details are in docs/ambient-meteors.md.


E047-13 adds an optional application-owned audio adapter. Build with
--features desktop-audio, then set client.audio.meteors true (default false).
Volume is client.audio.meteor_volume (0..1, default0.25). Breakup cues use
physical-distance delay, bounded queues and world/focus/teleport invalidation.
No device setup runs on the renderer lane. See docs/ambient-meteors.md for
Rodio/CPAL licenses, synthesis, budgets and pending runtime verification.

E047 controls and twenty physics presets are documented in docs/commands.md.
Example: `sky meteors spawn cascade 42`. All presets use the same solver; names
suggest initial conditions, not guaranteed animations. Meteor preferences flow
through config/options and SettingsStore; runtime settings own scheduling/audio.
Renderer property formatting lives in renderer/settings/values.rs to respect
the 500-line limit. No HUD, server or save-format changes are involved.

Meteor diagnostics expose history/frame bytes, emitting fragments, CPU prepare
time and GPU submission/culling counters on demand. At most one pooled physical
history initializes per frame. GPU duration is explicitly n/a until instrumented;
no draw-count-to-performance inference is made. See docs/performance.md.

`spawn_random_bolid [N]` now supports an irregular server-side shower. Example:
`spawn_random_bolid 20`; default N=1, allowed 1..64. The desktop pending request
retains the count until the current sky view is published; terminal/admin paths
use the same count validation. See docs/commands.md for cadence, framing and
cancellation limits. This is separate from local E047 presets.

## Server bolid preparation

spawn_random_bolid [N] keeps the server-timed v19 event and its eight retained
effect slots. One application-owned bolid-history worker builds the pure
renderer evolution outside the drawing thread. Requests, results and outstanding
keys are bounded to eight; rendering uses nonblocking request/poll operations.
Completed histories must match world context, the entire event, radius and scale.
Removed events, world transfers and replaced diagnostic paths cannot receive
stale results. Once ready, a history is sampled at current server age, so it
does not restart late or desynchronize its sky emission and point light.
Closing the renderer drops both channel ends; the worker exits after its bounded
current job. No game/world/protocol effects execute in that worker.

The dev bolid fragments diagnostic also reports solid/grain type and projected
pixel separation from the brightest emitter. Dev bolid status includes the
cached number and duration of simultaneously luminous solid fragments; these
help distinguish missing fragmentation from unresolved distant fragments.

## Ground-impact event

Run `spawn_random_meteorid` from the client console with its owned local server,
or through the server admin console. The event is authoritative and leaves a
permanent collidable meteorite and geometric crater. Client presentation includes
seeded color/flicker, the separately moving fragment, a ground flash, dust/ejecta,
point illumination and optional desktop-audio sound with propagation delay.
There is no mining/inventory reward attached to the remnant. Restart both endpoints
for protocol 21. `start-earth.ps1` rebuilds the matching release executables.
Tracked engine bodies arrive in typed `ImpactState` protocol messages and are
decoded as `BodyState`; they are ServerOnly entity components, so client replica
schema registration remains limited to components carried by entity replication.
Initial terrain synchronization gates prediction; impact revisions gate replacement
geometry/collision, and the burst waits for the new presentation. The terminal
client consumes the same replica without GPU dependencies. Runtime visual/FPS and
multiplayer acceptance remain part of final verification; static compilation alone
does not establish their results.

Terrain/collision preparation consumes `TerrainInputs` containing a validated
`PreparedTerrainSource` and separate world-bound overlays. Geographic sampling
remains immutable; resolved material-profile identity and crater/remnant revisions
invalidate prepared scenes. Excavation geometry, material cross-sections and water surfaces
are prepared on workers and published with matching collision and GPU state.
Water-only updates coalesce behind an active preparation instead of cancelling
it every solver tick; the last complete water frame remains presentable while
the latest one is prepared. Structural terrain and origin changes still cancel
stale work. See `docs/excavation.md` for controls and deferred acceptance.
