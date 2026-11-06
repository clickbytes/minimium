# minimium-client

## Purpose / Non-goals

Owns the desktop and text client application shells, input, console, renderer and command presentation. It consumes session, connection and clock synchronization contracts from `minimium-client-runtime`.

## Public API

Executable entry point is `src/main.rs`; internal modules provide startup and front-end behavior.

## Dependencies

The default `desktop` feature enables renderer GPU code and the astronomy,
window and clipboard adapters. `cargo build -p minimium-client --no-default-features`
builds a terminal-only executable: it defaults to console mode, skips material
cooking and has no wgpu/winit/astronomy dependency. Renderer quality/debug values
remain CPU-only public contracts when its `gpu` feature is disabled.

Consumes renderer, client-runtime, core, environment, world, assets, asset-io, astronomy, platform, protocol, procgen, sectors, telemetry and sample-game content, plus GPU/window libraries. Headless server and ctl must not depend on this package.

## Data flow

Front ends collect input and poll sessions, then build presentation state and submit frames to the renderer. Text mode uses the session without GPU construction.

## Ownership and execution

The front-end loop owns UI and polls the runtime connection controller; the runtime owns its socket, bounded protocol state and local-server guard, while background connector work reports typed results.

## Invariants and errors

Preserve desktop/text command behavior and keep protocol/session handling out of rendering algorithms.

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
executable with `materials/sample-v1` using `tools/package_client.py`.
`albedo` retains shaded uniform grey; `base_color` shows actual material color
without lighting, and `coverage` shows a fixed six-role palette without textures.

Client command groups declare specs in `src/client_commands/`; matching and alias resolution use `minimium-commands::CommandRegistry`. Client action execution remains here.

The client property catalog is registered beside those command groups. It owns live client presentation preferences, local movement input, and the replicated world-time view. Legacy camera, movement, and time commands share property validation and owner handlers. The desktop renderer reports requested/effective values and GPU fallback; the text frontend reports the same requested value and source with effective state marked unavailable when it has no renderer. Help, list, and completion read these registered descriptors, including ranges and enum values.

Client preferences use schema v1 RON and resolve default, file, mapped environment, then CLI input. `config validate` and `config show` run before renderer startup. `config save PATH` writes only persistent presentation settings and uses an atomic replacement. Session values reset to the resolved startup layer and are never written to the source file automatically.

Local server startup passes a typed launch configuration to client runtime. The runtime asks the sibling server executable for offline `config show --sources --format json`, validates local bind/headless constraints, and uses the same captured environment and argv settings for launch. Explicit startup settings prevent attach to an existing server; `--server-data-dir` remains a target selector.

## Change guide

Change window and platform startup here. Change protocol, connection, clock session, fixed-step prediction and snapshot reconciliation in client runtime. The runtime publishes a GPU-independent `ClientView`; the app maps its world, position, health and readiness values into `RenderFrame::client` alongside camera, environment and console data. `ClientGameState` explicitly owns current instance, manifests, session context and a `GameplayView`; the gameplay view owns prediction, health, collision, reconciliation and remote-player geometry. `Renderer` remains the host for connection, input, console, GPU resources, world catalog and overview presentation. `window_event` routes keyboard, mouse, wheel and cursor input through `renderer/window_events.rs`, which keeps the existing modal priority and key bindings. `session/message_handling.rs` routes server messages to connection, gameplay and transition handlers. `session/sector_readiness.rs` owns scene preparation, collision readiness and `TransitionReady` queuing, while the GPU adapter owns upload and draw. The app owns frame scheduling, the surface handle/configuration, presentation-worker preparation, visibility/caster selection and remaining GPU buffers. Renderer owns surface-image acquisition and presentation operations, camera/frame contracts, world/terrain/UI vertex data and material layouts, sky/Sun/marker/world pipelines, main scene and shadow depth pass encoding, cloud and atmosphere pass encoding, shader sources, planet visibility/LOD, clear-air optics/LUT generation, visual quality profiles, shadow targets and GPU owner, star catalogue/pipeline, linear output encoder, shared planet LOD selection, overview geometry buffers, world geometry GPU catalog, UI resources, cloud frame uploads, Milky Way texture, shared frame bindings, celestial halo/Moon pipelines and final UI/output pass encoding.

## Verification

Run `rtk cargo check -p minimium-client` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.


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
This requires protocol v16 on both client and server; existing saves are unchanged.

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
Both client/server must be restarted for protocol v17. Terrain status reports
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
