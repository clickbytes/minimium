# Weather visual-first implementation — P1 (canonical density)

## Delivered code
- Model revision 3: deterministic, camera-independent 3D erosion inside the existing transported CloudLobe geometry.
- CPU `detail.rs` and WGSL `cloud_detail.wgsl` share the lattice hash, cloud-local coordinates, two noise frequencies, erosion and falloff.
- Both local ray extinction and globe-derived column moments use six-point bounded quadrature over the same density; rain, thunder and shadows keep the published physical endpoints.
- Self/neighbor sunlight shadow uses the original coarse analytic approximation to avoid magnifying cost; visual agreement is approximate, not identical lighting.
- No new weather seed, clock, network message, checkpoint, global simulation or file format.

## Explicit limitations
- Erosion redistributes density without exact per-lobe volume normalization. Mass and optical depth may differ slightly from model revision 2; this is a visual prototype, not a conservative fluid solver.
- Remote spherical aggregate geometry remains approximate at its current LOD.
- Far-view temporal reconstruction and true fullscreen raymarching are not introduced in P1.
- Source-only review: no unit/integration/E2E tests or runtime GPU timings. This follows `AGENTS.md`; manual visual approval precedes the performance phase.

## Next stages
- P2: selectable hybrid local volume / curved orbital representation, render-quality controls and old-path fallback.
- P3: explicit teleportation frame identity and temporal-history invalidation.
- P4: stateful, bounded meteorological solver with an authoritative server publisher.
- P5: planet atmosphere profile with Earth as the first implementation.
