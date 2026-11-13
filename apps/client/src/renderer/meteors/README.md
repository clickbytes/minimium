# Local meteor lifecycle

The client owns monotonic event ages, bounded manual presets and world-scope
invalidation. `MeteorRuntime::build_frame` reconstructs a complete `MeteorFrame`
on every frame; it is a snapshot, not an incremental event stream. Elapsed trail
capture times must therefore be submitted on every frame, including after
skipped frames and floating-origin changes. The eight deterministic samples per
event do not accumulate allocations or duplicate entries in a frame.

`ActiveEvent::init_basis` scales its seeded direction by physical entry speed
and initializes one bounded `Evolution`. The CPU renderer kinematics model
advances mass, drag and deposited energy in fixed substeps; frame samples
interpolate its cached physical history. Photometry supplies core radiance and
the same source power for the future lighting consumer. Solid and luminous
radii are distinct. No per-frame integration or history allocation is needed.
See `docs/ambient-meteors.md` for limits and physical approximations.

`spawner::Spawner` supplies automatic candidates from counter-based, jittered
monotonic slots, independently of manual requests and visibility. See
`docs/ambient-meteors.md` for seed, catch-up, interval and startup controls.
The event vector is allocated once at the shared eight-event capacity.

No protocol, save, server simulation or GPU resource is owned here.
