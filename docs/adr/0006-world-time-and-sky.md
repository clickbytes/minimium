# ADR 0006: World time and sky

- Status: Accepted for Epic 004 implementation
- Date: 2026-10-30
- Owners: Epic 004
- Scope: shared world clock, astronomical observer model, environment data ownership, rendering boundary, protocol and save evolution

## Context

Minimium runs authoritative simulation on a fixed-step server, but the current
`Snapshot.server_time_ms` is elapsed monotonic process time. Missed simulation
ticks are skipped under load, so tick count and accumulated simulation deltas
cannot provide a calendar clock. At the implementation baseline, the renderer
had static lighting and a fixed sky background, and the wire format was protocol
v11 and the persistent world snapshot was save v7.

Earth is represented by a 650 km game radius and a reflected camera view. Its
geographic mapping is `latitude = asin(y / r)` and
`longitude = atan2(z, x)`. The game radius is a presentation and simulation
scale; it is not the physical Earth radius used by astronomical calculations.
The vertex format already carried normals, though the baseline cube helper
derived inward-facing normals. E004-09 corrected those normals.

## Decision

### Clock authority and semantics

- The server owns one clock with `ClockId = 1`, shared by loaded planet and
  interior instances. Environment profile is separate from immutable world
  geometry and its manifest.
- `RealTime` tracks server UTC. `TimeScale` starts from an explicit world date
  and advances from a monotonic anchor at a nonnegative rate. A rate of zero
  pauses the environment. `TimeScale` at 1x does not mean current UTC.
- Supported world time is POSIX UTC milliseconds in the half-open range
  `[2000-01-01T00:00:00Z, 2101-01-01T00:00:00Z)`. Scales use parts per million:
  1x is 1,000,000 and the maximum 1440x is 1,440,000,000. Arithmetic is checked
  in `i128`; absolute dates are not sent to GPU as `f32`.
- Rate changes and switches into `TimeScale` establish a new anchor at the
  currently evaluated world time. Setting a date is explicit. Switching into
  `RealTime` anchors to the adapter's current UTC and can therefore jump to the
  present by design. Clock revision, model revision, server session nonce and
  catalogue identity are distinct values. Physical simulation, cooldowns and
  real-time deadlines remain unscaled.
- Offline policy is explicit. The default is pause. Advancing offline applies
  only to `TimeScale`, uses nonnegative wall-clock elapsed time, and reports a
  backwards wall-clock observation rather than wrapping or saturating.

### Core time API and exact semantics

`minimium-core::time` exposes validated `ClockId`, `WorldTimeMs`,
`ClockRevision`, `SessionNonce`, `MonotonicMs`, `TimeScalePpm`, `ClockMode`,
`OfflinePolicy`, `ClockAnchor`, `ClockSample` and `SavedClockState`. Their raw
fields stay private; constructors and `TryFrom` reject unsupported codes and
values. MVP accepts only clock id 1, mode codes 0/1, policy codes 0/1, model
revision 1 and catalogue id 1. A real-time anchor must carry the effective 1x
rate. Catalogue SHA-256 remains opaque: core does not reject or manufacture a
digest based on its byte pattern.

`evaluate_clock(anchor, monotonic_ms)` checks monotonic order before subtracting,
then computes `world_utc_ms + floor(delta_ms * rate_ppm / 1_000_000)` from the
original anchor using checked `i128` arithmetic. `sample_clock` binds the
resulting UTC to the exact monotonic timestamp and session nonce;
`validate_sample_after` compares monotonic values only within one nonce.
`set_world_time`, `set_time_scale`, `set_clock_mode` and
`set_offline_policy` return a complete replacement anchor. They first evaluate
the previous anchor, increment its revision with checked addition, and leave
the original unchanged on error. Repeating a mutating command still increments
the revision. A new clock starts at revision 1. Switching to RealTime requires
current UTC from the adapter; switching to TimeScale preserves the current date
and existing scale, or uses 1x when coming from RealTime. Setting a date switches
to TimeScale, preserves an existing TimeScale rate, and otherwise selects 1x.

`save_clock_state` stores the evaluated world date, mode, rate, policy and
revision. Both saved UTC fields use the same supported `WorldTimeMs` range.
Restore preserves the saved revision; the new process nonce distinguishes the
new monotonic domain. RealTime restore uses supplied current UTC, TimeScale/Pause
uses the saved world date, and TimeScale/Advance adds only nonnegative wall-clock
elapsed time at the saved rate. A backwards wall clock is reported in the
restore result and contributes zero elapsed time. POSIX values do not represent
leap seconds; parsing and civil-time conversion remain outside core.

### Data ownership and dependencies

- `minimium-core` owns validated, platform-neutral time and environment types
  plus pure anchor evaluation, mutations and restore. It has no clock I/O,
  randomness, protocol, GPU, logging or astronomy backend dependency. The
  concrete API and rounding/revision semantics are recorded above.
- The server owns UTC/monotonic adapters, the authoritative anchor, admin
  mutations, persistence snapshots and synchronization samples. It never
  scales simulation time.
- The protocol transports bounded, versioned clock/environment state; the
  client estimates server time from monotonic samples and never trusts its
  system timezone to determine sky position.
- The client astronomy module owns observer conversion, ephemerides and
  `SkyState`. Astronomy Engine C v2.1.19 is pinned at
  `61dc07020aaa6885d2c7f688a4d82beaf6edb9ef` behind a narrow safe Rust API.
  C/FFI stays out of core, server and protocol. Offline calculations use f64;
  only bounded directions and presentation values reach GPU buffers.
- Astronomy Engine source is MIT licensed. HYG 4.1 data and derived catalogue
  are CC BY-SA 4.0; retain attribution, source revision and share-alike terms
  alongside the generated asset. These are separate source and data license
  records.
- `minimium-assets` owns the deterministic HYG 4.1 catalogue, pinned at
  `c7f7f883fe678cc7680169a50ccd7dcc49b060ce`, with catalogue id 1 and a
  measured SHA-256 of the generated asset. Selection is exactly 500 global
  stars ordered by apparent V magnitude and HYG id. Data attribution and
  CC BY-SA 4.0 terms are retained separately from code licensing.

### Observer and coordinate frames

Earth geography uses the existing `+Y` north-pole axis and `atan2(z, x)`
longitude convention. E004-06 owns one conversion into a documented local
north/east/up frame and the renderer reflection. Physical Earth radius is used
for astronomy and lunar parallax; game radius and relief remain presentation
geometry. Height conversion is explicit and is not inferred from the 650 km
radius. Procedural planets use an explicitly named EarthLike profile.

`SkyState` separates global/geocentric directions from topocentric observer
directions and geometric from apparent directions. Sun and Moon use the
appropriate observer correction; star records are individually propagated for
proper motion before orientation. One `EnvironmentFrame` supplies the same
astronomical state to sky, terrain, water and globe shading.

### Versioning and persistence

- The integrated implementation uses protocol v12 and save v8, delivered by
  E004-04 and E004-03 respectively. Protocol v11 peers are rejected.
- Save v8 stores a clock anchor and offline policy without process nonce or
  monotonic timestamp. Migration preserves all existing player, manifest and
  sector data; malformed or mismatched snapshots fail without clearing user
  data.
- Protocol v12 carries clock identity/revision, session nonce, sample UTC and
  matching monotonic time plus environment/catalogue identifiers. State is sent
  on join and world transfer, and on change and periodic synchronization.
- Clock changes never alter map fingerprints, seeds, generator revisions,
  manifest content revisions or sector state.

### Rendering scope

`EnvironmentFrame` drives day/night, twilight, sky colour, star visibility,
Sun/Moon direction and scene/globe lighting. Quality profiles alter rendering
cost only: `low` has no shadow map; `standard` uses one local 1024² map with a
128 m range. Local caster coverage is bounded and reported; this does not claim
complete planetary terrain shadows. Full atmospheric scattering, weather,
clouds, planetary system bodies, eclipses and cascaded global shadows are out
of scope.

## Alternatives considered

- **Derive time from simulation ticks or accumulated frame deltas:** rejected;
  skipped ticks, variable redraw and client stalls would make calendar time
  drift.
- **Use each machine's local clock/timezone:** rejected; clients would show
  different skies and daylight-saving changes would move the Sun.
- **Put time in `WorldManifest` or terrain fingerprints:** rejected; time is
  mutable environment state and must not invalidate immutable generated worlds.
- **Implement approximate ephemerides in core:** rejected for v1; it broadens
  the dependency-free core and puts accuracy responsibility on game formulas.
- **Link Astronomy Engine into core/server:** rejected; C toolchain and FFI
  complexity would cross server and core boundaries without benefit.
- **Send star or ephemeris data on every snapshot:** rejected; clients can
  calculate locally from small synchronized clock samples and a pinned asset.
- **Use real-Earth scale for game geometry:** rejected; it would conflict with
  the established world scale and simulation assumptions.

## Consequences and risks

- Requires checked time conversions, protocol v12 and save v8 migrations.
- Windows target `x86_64-pc-windows-msvc` compiled and linked the pinned C
  backend in E004-07 (PR #77), including a workspace release build. The earlier
  E004-00 shell PATH did not expose `cl.exe`; that historical observation is not
  the current build status. See the E004-15 audit for the fresh compiler capture.
  Windows needs MSVC or compatible clang-cl plus the SDK/libraries; Linux needs
  GCC/Clang and the C development toolchain. Linux has not been built in this
  audit. Ordinary builds need neither bindgen nor libclang. C remains absent
  from the server/core dependency path.
- HYG-derived asset redistribution requires correct attribution and share-alike
  licensing. The generated asset hash must be measured, never fabricated.
- Small server-clock corrections can be smoothed; large corrections and
  explicit admin sets create a new clock revision. Clients discard stale
  revisions and stale session nonces.
- A reduced game planet and terrain relief affect visible horizon timing even
  when astronomical directions are correct. Accuracy claims distinguish sky
  direction from terrain occultation and atmospheric appearance.
- Functional, visual, network and performance acceptance remains deferred to
  E004-16 and the project's explicitly authorized final verification phase.

## Implementation ownership

| Issue | Decision owner / delivery |
| --- | --- |
| E004-01 / #53 | Core clock/environment types and pure evaluation |
| E004-02 / #54, E004-03 / #55 | Server clock, commands, save and migration |
| E004-04 / #59, E004-05 / #60 | Wire synchronization and client estimator/lifecycle |
| E004-06 / #56, E004-07 / #57 | Observer conversion and astronomy backend |
| E004-08 / #58 | Star catalogue data and attribution |
| E004-09..14 / #61..66 | Shared renderer frame, sky objects, lighting, shadows and quality |
| E004-15..17 / #67..69 | Audit, final verification and final integration |

Changes to these shared contracts update this ADR, `epic-004-contracts.md`
and every affected issue in the same integration increment.
