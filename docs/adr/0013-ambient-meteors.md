# ADR 0013: Ambient cosmetic meteors — baseline, contract and domain boundaries

- Status: Accepted for the E047 design baseline; implementation follows E047-01..17
- Date: 2026-11-08
- Owners: E047-00 / Epic #366 ([EPIC E047](https://github.com/clickbytes/minimium/issues/366))
- Scope: local cosmetic meteor presentation contract, units, clocks, GPU budget, ownership and pass order. This ADR freezes decisions only; it introduces no shader, runtime, event, world-entity, save or protocol change.

## Context: baseline audit at E047-00

Recorded at the read HEAD of `codex/epic-008-config-cli-console`
`18b970e102cf0fb2ae6b0e7dd6e996436e09fe76` (2026-11-08, "add sky bolid spawner"):

| Item | State at HEAD |
| --- | --- |
| Wire contract | protocol v19, `PROTOCOL_VERSION` in `crates/protocol/src/limits.rs` |
| Save format | v10, `SAVE_VERSION` in `crates/storage/src/format.rs` |
| Generation | revision 5 enforced by `generator_contract` in `crates/procgen/src/generator_contract.rs` |
| Working tree | clean except untracked `logs/`; no E047 work has started; `docs/architecture.md` body text still mentions protocol v18 in prose — stale, superseded by the wire constant above |
| Cloud inputs | immutable `CloudSnapshot` publications from a bounded weather worker (`docs/weather.md`) |
| HDR path | pre-exposed linear scene target + single tone mapping in `OutputEncoder`; `MINIMIUM_FORCE_HDR_FALLBACK=1` selects the analytic fallback |

Existing sky effects that this ADR must coexist with (all verified at HEAD):

- **Server-timed bolid demo.** Clients publish `ClientMessage::SkyView`
  (`crates/protocol/src/models.rs`) at most once per second
  (`apps/client/src/renderer/bolids.rs::send_sky_view`); `crates/server-runtime/src/runtime/bolid.rs`
  schedules or accepts manual requests (`spawn_random_bolid` cooldown of five seconds) and sends
  `ServerMessage::Bolid { context, event: BolidEvent }` to players within 250 km of the spawn.
  `BolidEvent` carries the *server-monotonic* `start_monotonic_ms`, world-space `f32` endpoints,
  seed, `duration_ms` and kind 0/1/2. Clients retain at most eight events
  (`MAX_BOLIDS` in `crates/renderer/src/milky_way_gpu.rs`), age them by
  `session.server_monotonic_ms()` with an `Instant` fallback, and pack one `BolidGpuFrame`
  (14,608 bytes) per frame.
- **Renderer integration.** `GpuResources::render` (`apps/client/src/renderer/render.rs`) selects
  `bolids::strongest_bolid_light` into the single `EnvironmentUniform::bolid_position_intensity`
  / `bolid_rgb` slots (880-byte offset in the 928-byte uniform, `crates/renderer/src/gpu_frame.rs`),
  uploads the frame via
  `MilkyWayGpu::upload_bolids`, then `frame_encoder::encode_frame` orders: shadow depth, atmosphere
  LUT prepare, main scene, atmosphere RGB composition, `bolid_dust::encode`, clouds OIT,
  `bolid_dust::resolve`, final passes (`crates/renderer/src/frame_encoder.rs`).
  `bolid_light.rs::incident_luminance` and `exposure_scale` meter the point flash.
- **Sky demo and quality.** `sky demo on|off` toggles the demo solar-system catalogue
  (`demo_universe_enabled`, `apps/client/src/renderer/settings.rs`);
  `sky quality low|standard` selects `SkyQuality` profiles (`crates/renderer/src/quality.rs`).
  `--demo`/offline play uses the explicit `LocalDemoEnvironment` adapter
  (`apps/client/src/environment/demo.rs`); network sessions never fall back to it.
- **Environment conditions.** The client-runtime session publishes an immutable
  `EnvironmentFrameInput` (`crates/client-runtime/src/environment.rs`): estimated/render world UTC in
  milliseconds, descriptor, `WorldContext`, clock revision and rate parts per million. The renderer
  adapter folds it into the client frame (`EnvironmentFrame`,
  `apps/client/src/renderer/environment.rs`) through the observer path
  (`apps/client/src/renderer/environment/observer.rs`), which already fails with explicit states
  (`ObserverError::EnvironmentProfileMismatch`, `InvalidPlanet`, `MissingPrecisePosition`) and
  produces `EnvironmentFrame::unavailable` for dungeon/interior and missing-observation cases.

Design problem: Epic E047 wants sporadic ambient meteors and rare bright bolides that are visible
during normal first-person play, are never simulated server-side, and still share the existing sky,
cloud and lighting presentation. The rendering contract must be producer-neutral now so a future
tracked meteoroid adapter can reuse it without re-opening protocol, save or server decisions.

## Decision

### 1. Standard meteors are local cosmetic effects only

- Ambient meteors, dust, fragments, trails and their light do not touch the server tick, world
  entities or components, replication, collision, storage, the protocol version or the generator.
  They are purely cosmetic: never authoritative for gameplay and never balancing inputs.
- They carry no saved or persisted state, and no protocol message is added for them in any E047
  issue. A client without a session (demo or dropped connection) still runs the local system.
- The contract's `MeteorSource` tag distinguishes `Cosmetic` from `Tracked`. `Tracked` is an input
  marker only: this epic designs no tracked producer, no orbits and no adapter beyond reserving the
  same `MeteorFrame` input for a future producer.

### 2. Ownership map and flow (owner → producer → consumer)

~~~text
conditions: world UTC, sky, quality, clouds            lifetime/animation: client monotonic
        |                                                           |
        v                                                           v
[client-runtime  EnvironmentFrameInput]          [apps/client  renderer::meteors  (E047-02/04)]
  render_world_utc_ms, context, rate               local lifecycle: rate, seed, TTL,
        |                                          dev commands via client_commands
        v                                                    |
[apps/client  renderer::environment]                         v
  EnvironmentFrame, ObserverAdapter  ---- spawn gate ---->  [crates/renderer  meteors  (E047-01)]
  (explicit unavailable/interior states)    f64 physical metres -> adapter -> f32 origin-relative
                                                          |
                                    MeteorFrame (CPU-only, bounded, non-allocating)
                                                          |
        +-------------------------+-----------------------+------------------+
        v                         v                       v                  v
[TransientLightUniform]    [meteor emission layers]   [dust/trail layers]  [sky diagnostics]
 shared frame bind group    finite-distance layer      finite layer          sky info/status
 (E047-09)                  after atmosphere           around cloud OIT      (E047-14)
        |                         (E047-03/12)             (E047-03/08)
        +------ world/terrain/ocean/clouds WGSL via shared group ------+
                                  v
                       OutputEncoder: one tone mapping pass
~~~

Ownership table:

| Responsibility | Owner | Package |
| --- | --- | --- |
| CPU-only meteor contract: `MeteorSource`, `MeteorSample`, `MeteorTrailSample`, `TransientLightSample`, `MeteorFrame` | renderer | `crates/renderer/src/meteors` (new in E047-01; files under `crates/renderer/src/`, no new Cargo crate) |
| Physical-to-render coordinate adapter (metres, f64, origin-relative f32) | renderer | `crates/renderer/src/meteors/coords.rs` (E047-01) |
| GPU transient-light uniform and shared bind-group extension | renderer | `crates/renderer/src/gpu_bindings.rs`, `gpu_frame.rs`, `shaders/transient_lighting.wgsl` (E047-09) |
| Emission/dust/trail passes inside the existing frame order | renderer | `crates/renderer/src/frame_encoder.rs`, `bolid_layer.rs`-style finite layers (E047-03, -07, -08, -12) |
| Local lifecycle: spawning, rate, seed, fallback, expiry | client | `apps/client/src/renderer/meteors` (new adapter module, E047-02/04; planned path already named by Epic #366) |
| Developer commands and properties (`sky meteors ...`) | client | `apps/client/src/client_commands/` (E047-14), registered beside the existing environment commands |
| World UTC, observer and weather conditions | environment (unchanged) | `crates/environment/src/{time,sky,weather}`; no meteor vocabulary is added here |
| Session clock sync (already existing) | client-runtime (unchanged) | `crates/client-runtime/src/environment.rs`, `session/clock.rs` |

The existing server-timed bolid demo path is *not* rewritten: `BolidEvent`, `server-runtime`
scheduling and the single `EnvironmentUniform` bolid light slot stay as they are. A later adapter
could feed a server-timed event into the same `MeteorFrame`, but that adapter is not part of E047
and its absence leaves no dead placeholder.

### 3. Typed contract (described here, defined in code by E047-01)

The contract is CPU-only and has no `wgpu` or window dependency, so the terminal
`--no-default-features` client can still carry it:

- `MeteorSource { Cosmetic, Tracked }` — producer semantics tag carried by every sample.
- `MeteorSample` — identity, client-monotonic start, position in physical metres (`f64`) in the
  agreed anchor frame, velocity in m/s (`f64`), emissive linear RGB and intensity, effective radius,
  lifecycle phase and source tag. Constructors validate finite, nonnegative and bounded values and
  reject anything that the renderer cannot represent.
- `MeteorTrailSample` — trail geometry and opacity samples with an explicit monotonic timestamp.
- `TransientLightSample` — position, luminous intensity/irradiance, linear colour, actionable range
  and time-to-live for one emitting center.
- `MeteorFrame` — non-allocating per-frame view with fixed buffers: at most 16 bright cores, and at
  most 4 transient light sources; deterministic ordering by observer-visible impact with identity
  tie-breakers. Zero active meteors is a valid frame that costs no extra GPU work.

`Tracked` never implies a server producer: no message, entity or save field backs it.

### 4. Units, scales and degraded states

| Quantity | Unit / representation | Rule |
| --- | --- | --- |
| Physics: positions, velocities, sizes | physical metres, `f64`; velocities m/s | one conversion, in the coordinate adapter only |
| Game world anchor | world metres (game scale, `f64` arrays from `RenderViewFrame`) | the game planet scale comes from the world manifest, never a constant: `planet.radius_mm` and `height_scale_permille` |
| Optical planet | 6,371 km Earth radius for clear-air optics (`atmosphere::EARTH`) | used by shared optics; never mixed with the game planet radius |
| GPU positions | `f32` metres relative to the current render origin | large f32 absolute differences are never taken; the source f64 world anchors are subtracted in the adapter each frame |
| Emission | linear RGB radiance/irradiance before the single exposure | HDR and fallback share the same scene units |
| Lifetimes and animation | seconds on the client monotonic clock | never world UTC, never server monotonic |
| Per-frame caps | at most 16 cores, 4 lights per frame | hard contract limits; no magic per-call-site numbers |

The height-scale conversion already used by the observer path
(`camera_reference` in `apps/client/src/renderer/environment/observer.rs`, and by
`prepare_bolids` length scaling) is the same physical↔game mapping; the meteor adapter must reuse
that convention instead of inventing a second one.

Degraded states are explicit and immediate: missing context or world UTC
(`EnvironmentFrameInput::unavailable`), interior/dungeon profile (`EnvironmentFrame::interior`,
`resolve_sky_model == SkyModel::None`), missing observer (`ObserverError` states), missing planet
definition (`InvalidPlanet`). Each turns the meteor system into a zero-cost no-op without panics,
NaNs or new resource allocation; recovery follows the already published `EnvironmentFrame` flags.

### 5. Clock model

| Clock | Owner / source | Used for | Explicitly not used for |
| --- | --- | --- | --- |
| Client monotonic (`Instant` epoch; precedent: `ANIMATION_EPOCH` in `crates/renderer/src/gpu_frame.rs`, `LocalDemoEnvironment.started_at`) | client process | meteor lifespan, visible animation phase, spawn-rate windows, dev freeze/replay | exposure gating |
| World UTC (`render_world_utc_ms` + clock revision from `EnvironmentFrameInput`) | client-runtime session estimator | spawn gating (solar altitude, moon phase, weather limits), sky/constellations, weather | any lifetime accumulation |
| Server monotonic (`session.server_monotonic_ms()`) | protocol estimator | existing `BolidEvent` ageing only | new meteor timing |

Consequences:

- A 1,000,000× time scale, a date jump or an offline `TimeScale` free of world UTC changes cannot
  create event explosions: meteors expire by monotonic age, and spawn budgeting owns a fixed
  per-window rate against the client clock. Pausing or accelerating UTC only changes *conditions*.
- Reconnect drops the estimator and every pending meteor instantly; the state is client-local and
  there is nothing to reconcile.
- Teleport/portal/origin rebase re-derives per-frame positions from f64 anchors minus the current
  render origin; distance and altitude culls then naturally drop far-away sources.

### 6. Budgets, GPU fallback and lighting

- Hard caps: ≤ 16 bright cosmetic cores rendered per frame, ≤ 4 transient light sources uploaded in
  the GPU uniform, and a fixed number of trail/dust segments and history samples per meteor (bounded
  at contract time, E047-01); one bounded optional worker for pure CPU preparation at most; the
  render path itself allocates nothing per frame.
- The GPU path stays one pre-exposed linear HDR composition with a single tone mapping;
  `TransientLightUniform` rides inside the shared frame bind group (`@group(0)`, currently bindings
  0/1 as of HEAD; the exact new slot is verified when implemented — planned 11, must still be free
  then). Unsupported devices keep the analytic fallback path used by the bolid feature; a missing
  transient-light uniform degrades to sky-only meteors, never to a broken frame.
- Multi-source flash metering reuses the existing bounded metering rule
  (`bolid_light::exposure_scale`, `incident_luminance`) with the source ranked by received
  luminance; meteors never bypass the shared exposure path with a global full-scene
  brightening.
- Clouds remain surface faces and weighted OIT; transient cloud lighting is an analytic local
  radiator — a measured approximation of a point light scattering, not a full radiative
  transport solver (see `docs/weather.md` for the current cloud budget).
- Declared approximations: ablation/emission photometry and fragmentation are visual models (already
  documented for bolides), atmospheric extinction reuses the shared optical profile and finite
  sample rules, and no CFD/plasma/thermochemistry claims are made.

### 7. Frame pass order and integration points

The current, immutable-for-this-epic order in `frame_encoder::encode_frame`:

1. shadow depth pass (may be disabled);
2. atmosphere LUT preparation (only when the scene is visible);
3. main scene pass: opaque world, sky, stars, celestial bodies;
4. clear-air atmosphere RGB composition;
5. `bolid_dust::encode` finite-distance layer;
6. cloud pass (weighted OIT);
7. `bolid_dust::resolve`;
8. UI pass, linear→sRGB scene output encode, UI output pass.

Meteor-specific passes must slot into this order using the same finite-distance layer pattern: sky
emission composes after the clear-air atmosphere, dust/trail layers bracket the cloud pass the way
`bolid_layer`/`bolid_dust` currently do, and transient lighting enters through the uniform *before*
the passes that consume light. Passes are owned by fixed encoder structures declared in the
renderer; there are no new pipeline families and no per-meteor pipeline creation.
`MAX_HALO_BODIES` and the bounded halo array of `gpu_frame.rs` are the existing bounded-array
precedent this contract follows.

### 8. Risk table

| Risk | Resolution | Where it is anchored |
| --- | --- | --- |
| Event explosion at 1,000,000% time scale, date jump, teleport or reconnect | lifetime on client monotonic; world UTC only gates conditions; reconnect clears locally; see § 5 | contract § 5; runtime files planned in `apps/client/src/renderer/meteors` |
| Missing planet, observer, context or valid UTC | explicit disabled states already published by the observer/environment path; zero-cost no-op | `observer.rs`, `EnvironmentFrame::status`, § 4 |
| Unbounded GPU/CPU cost or stall | hard caps in § 6, deterministic priority, fixed buffers, no per-event GPU layout | `MeteorFrame` (E047-01), `TransientLightUniform` (E047-09) |
| HDR/fallback divergence | one pre-exposed path, one tone mapping, same scene units; fallback keeps analytic integrity | § 6; `OutputEncoder`, `bolid_light.rs` |
| Cloud/light misuse growing into global illumination | clouds stay surface/OIT; local analytic radiator; documented approximation | § 6; `docs/weather.md` unchanged |
| Server/protocol/save/world drift by accident | forbidden-change list in § 1 and consumer/producer ownership table | epic #366 contract; `architecture.toml` note |
| Double systems for the two producers | existing bolid path stays untouched; `MeteorFrame` is the single shared input seam for any future producer | § 2; `BolidEvent` stays in protocol v19 unchanged |
| Command/UI drift | new commands only through the existing registry/property catalog; `sky demo`, `sky quality`, `weather clouds ...` keep their semantics | E047-14; `apps/client/src/client_commands/` |
| Stale baselines (files move) | every E047 issue re-reads `AGENTS.md`, `architecture.toml`, owner README and HEAD; reopen-rule in the epic | epic #366 execution rules |

### 9. E047 execution sequence

Linear, one issue at a time; the gate and evidence are per-issue on GitHub. The next
issue begins only when the previous one is verifiably CLOSED:

| Step | Issue | Content |
| --- | --- | --- |
| 0 | [E047-00 #367](https://github.com/clickbytes/minimium/issues/367) | this ADR and the anchor audit (baseline) |
| 1 | [E047-01 #368](https://github.com/clickbytes/minimium/issues/368) | `MeteorFrame`, units, `renderer::meteors` contract |
| 2 | [E047-02 #369](https://github.com/clickbytes/minimium/issues/369) | local runtime, manual console spawn |
| 3 | [E047-03 #370](https://github.com/clickbytes/minimium/issues/370) | base shader, GPU resources, render pass |
| 4 | [E047-04 #371](https://github.com/clickbytes/minimium/issues/371) | automatic generator: rate, seed, lifecycle |
| 5 | [E047-05 #372](https://github.com/clickbytes/minimium/issues/372) | trajectory, ablation, photometry |
| 6 | [E047-06 #373](https://github.com/clickbytes/minimium/issues/373) | fragmentation, dust, cascades, flashes |
| 7 | [E047-07 #374](https://github.com/clickbytes/minimium/issues/374) | fading and persistent trails |
| 8 | [E047-08 #375](https://github.com/clickbytes/minimium/issues/375) | visibility: terrain/cloud/planet occlusion |
| 9 | [E047-09 #376](https://github.com/clickbytes/minimium/issues/376) | shared `TransientLight` API and GPU uniform |
| 10 | [E047-10 #377](https://github.com/clickbytes/minimium/issues/377) | dynamic light on terrain, objects, water |
| 11 | [E047-11 #378](https://github.com/clickbytes/minimium/issues/378) | cloud light and self-shadowing |
| 12 | [E047-12 #379](https://github.com/clickbytes/minimium/issues/379) | atmosphere, night exposure, halo |
| 13 | [E047-13 #380](https://github.com/clickbytes/minimium/issues/380) | optional delayed sound (client-only) |
| 14 | [E047-14 #381](https://github.com/clickbytes/minimium/issues/381) | commands, settings, 20 dev presets |
| 15 | [E047-15 #382](https://github.com/clickbytes/minimium/issues/382) | budgets, LOD, fallback, diagnostics |
| 16 | [E047-16 #383](https://github.com/clickbytes/minimium/issues/383) | final source review and static sweep |
| 17 | [E047-17 #384](https://github.com/clickbytes/minimium/issues/384) | runtime validation with explicit GO consent |

## Alternatives considered

- **Ambient meteors as replicated events (a second `BolidEvent`-style message):** rejected. The
  server would spend tick budget on pure presentation, clients would replay through reconnect
  windows, and protocol would grow for zero gameplay value. Server-timed spectacle already has an
  exact, existing home in the bolid demo.
- **Ambient meteors as world entities (replication, save, collision):** rejected. Crossing the
  "local effects ≠ world objects" boundary would pull simulation, storage and schema costs into
  every meteor without any gameplay benefit.
- **Deriving lifetime from world UTC (or server monotonic):** rejected. Accelerated clocks, offline
  restore and reanchoring would either freeze or explode lifetime; monotonic client time is
  already the precedent (`ANIMATION_EPOCH`, bolid `Instant` fallback).
- **Inventing a numeric game-planet radius for meteors:** rejected. Any hard-coded radius would
  break on authored planet definitions; the observer path already carries
  `radius_mm`/`height_scale_permille` and optical positions use `atmosphere::EARTH`.
- **Adding one new uniform per meteor or per source:** rejected — portability (16 KiB uniform cap)
  and pipeline permutations rule it out; a fixed bounded uniform with ≤ 4 slots matches the existing
  `HaloSource` pattern.
- **A new Cargo crate for the meteor contract:** rejected. The ownership split across
  `minimium-renderer` and `minimium-client` is already the dependency surface the flow needs; an
  extra crate would only duplicate boundaries without gaining isolation.

## Consequences and risks

- E047-01..17 implement against this ADR; discovering an inconsistency requires reopening E047-00
  (per the epic flow) rather than diverging locally.
- Exactly the files named in § 2 gain code in later issues; this issue changes documentation only,
  so no runtime behavior exists to verify yet.
- Static checks at this baseline document the untouched code; runtime performance, visual parity,
  cloud/extinction accuracy and sound are explicitly **not** claimed before the E047-17 gate, which
  itself waits for an explicit operator consent sign-off per `AGENTS.md`.
- `docs/architecture.md` prose still says protocol v18 in several pre-existing paragraphs; the wire
  constant is v19 at this HEAD. Existing paragraphs are left alone here; follow-up documentation
  hygiene is tracked by this ADR's context table rather than silently rewritten in an E047 doc.
