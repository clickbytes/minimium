# Hybrid Planetary Atmosphere — implementation handoff (P1–P5)

Scope: Earth only, visual-first. The user intentionally deferred P0 performance instrumentation
and will manually assess visual quality. All PRs remain draft until the review gates below.

## Stacked delivery (merge bottom to top only after acceptance)

| Stage | PR | Source changes |
|---|---|---|
| P1 | #153 | Deterministic eroded 3D cloud density, shared Rust/WGSL kernel |
| P2 | #154 | Near-volume detail normals and optical sampling, 2.5D orbital opacity, analytic fallback |
| P3 | #155 | View invalidation at teleport; world+seed+UTC identity, stale cut rejection |
| P4 | #156 | WeatherGrid 48×24×3 dynamics, atlas coupling, bounded server weather worker |
| P5 | #157 | Atmospheric profile, gas composition, unified optical constants and LUT |

The **tip branch** `feature/weather-p5-atmosphere-profiles` includes all preceding stages.
No merge to `main` has been performed as part of this implementation.

## Manual build and comparison

On a supported Rust toolchain installed per the repository's `rust-toolchain.toml`:

```text
git fetch origin
git switch feature/weather-p5-atmosphere-profiles
cargo build --workspace --locked
cargo run -p minimium-client
```

For Windows with the Earth asset package imported:

```powershell
./start-earth.ps1
```

Use F1 console while connected to the Earth world:

```text
weather simulated
weather clouds method volumetric
weather clouds quality high
weather globe natural
weather status
weather clouds status
map_telepo
```

For A/B of appearance while keeping source weather conditions, switch only
`weather clouds method analytic`, then back to `volumetric`. `quality high`
returns detailed lighting, but the render still draws cloud-lobe enclosures.
`weather globe readable` is an intentional artistic presentation override;
use `natural` for the physical correspondence comparison.

Manually inspect: clear/partly/stormy weather; day, dawn, dusk, full Moon;
the overhead view and cloud limb; under/inside/above the same local cloud;
teleport at stable UTC; roof-occluded rain; shadow movement; extreme
`time scale`; repeat with the same seed and GEBCO package.

Server `minimiumctl --data-dir PATH status` exposes the published Earth
weather epoch and quantized digest; `weather status` exposes client atlas
endpoint digests. Match seed, UTC endpoint and source asset before comparing.
Digests are diagnostic: they do not establish deterministic bit parity across
platforms. Client live forecast is intentionally a separate correction.

## Implemented approximations / constraints

1. A three-layer circulation model with bounded 6-hour physical cohorts and
   semi-Lagrangian transport is not a conserving atmosphere GCM.
2. Lobe cohort transport remains partially procedural; the global solution
   influences local cloud displacement, morphology, winds and condensate.
3. Local "volumetric" integrates eroded density through analytic ellipsoid
   screen enclosures. This is not a fullscreen single-ray cloud renderer.
4. Orbital cloud opacity derives from published column moments at finite
   resolution, not exact 3D projection. Teleport agreement is approximate.
5. The server worker reconstructs Earth weather from authoritative UTC
   asynchronously but does not yet transmit full weather frames to clients,
   persist an independent atmospheric state or handle multiple Earth worlds.
6. Only Earth is active for simulation and optics. Profile values for other
   planets do not imply support for their chemistry, condensation or ephemerides.
7. Runtime quality, frame times, visual correctness and WGSL validation have
   **NOT RUN** status. No unit/integration/E2E tests were run in accordance
   with `AGENTS.md`.

## Release gate after visual approval

After the user chooses to begin verification, perform cargo fmt/check/clippy,
Naga validation of all generated WGSL variants, GPU launch and captures,
teleport parity, save/protocol/clock regressions and perf budgets. Track
failures with their actual SHA and adapter details. Do not mark a draft PR
as tested, ready or merged based on source review alone.

Rollback: `git switch main` (untouched). Previous render mode within the
feature branch: `weather clouds method analytic` (only swaps near-cloud
view extinction, not the canonical physical model).

## Additional source-level appearance cleanup

- Cloud light uses a bounded forward-scattering phase with a stronger high-ice-cloud response rather than an arbitrary `pow(cos θ,10)` term. This changes image only, not source weather.
- Rain particles receive a deterministic per-cell glint amplitude and distinct Sun/Moon angular phase response, still gated by cloud shadows and local light. Snow avoids water-drop-like bright glints.
- Atmospheres without water condensable no longer invent water vapour/cloud condensate inside the generic weather solver. Earth still uses the same water cycle.
- Runtime rendering quality and GPU costs require the user's separate acceptance; these source changes do not claim measured quality or verified compilation.
