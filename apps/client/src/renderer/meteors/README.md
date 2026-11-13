# Local meteor lifecycle

The client owns monotonic event ages, bounded manual presets and world-scope
invalidation. `MeteorRuntime::build_frame` reconstructs a complete `MeteorFrame`
on every frame; it is a snapshot, not an incremental event stream. Elapsed trail
capture times must therefore be submitted on every frame, including after
skipped frames and floating-origin changes. The eight deterministic samples per
event do not accumulate allocations or duplicate entries in a frame.

`ActiveEvent::init_basis` multiplies its unit flight direction by the seeded
physical speed. `EventBasis::position_at` integrates that velocity in metres per
second. Parameters are diagnostic placeholders pending the ablation stage;
this initial linear model terminates at its lifetime or the planet datum.

`spawner::Spawner` supplies automatic candidates from counter-based, jittered
monotonic slots, independently of manual requests and visibility. See
`docs/ambient-meteors.md` for seed, catch-up, interval and startup controls.
The event vector is allocated once at the shared eight-event capacity.

No protocol, save, server simulation or GPU resource is owned here.
