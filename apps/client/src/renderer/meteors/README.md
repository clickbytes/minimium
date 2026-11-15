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


Fragmentation histories are pooled at startup (eight bounded Evolution owners)
and returned on every dismissal/clear path. `presentation::push_event` emits
solid fragments first, then grain packets into free frame slots. Identities
combine the event id and a never-reused body slot. Phase, photometry and short
wake positions sample each body independently; the process log is diagnostic
data and never creates world entities. `fragmenting` is a manual brittle-entry
preset; `fireball` retains material variability. The pure renderer model owns
mass transfers and pressure thresholds, while the client owns pool lifetime.

trains::Trains captures fixed-time f64 deposits into four 64-point rings;
trains::push selects at most eight points per retained trail owner and adapts
world coordinates afresh. Head presentation ends with Evolution; lifecycle
expiry waits for the last gas/dust TTL, then returns the pooled history.
