# Ambient cosmetic meteors

These are local presentation events, separate from the server bolid demo.
Two players are not guaranteed to see the same meteor. Neither this scheduler
nor its events enter world simulation, networking, saves or generation.

## Scheduler (E047-04)

The client captures one session seed at runtime creation (wall-clock nanoseconds
mixed with process identity). `MINIMIUM_METEOR_SEED=<decimal u64>` overrides it
for debugging; the world membership context is mixed into the seed when that
scope becomes valid. Same seed, context, monotonic epoch and observer inputs
reproduce the candidate sequence within this build. The context hash is not a
persisted or cross-version contract. No per-frame entropy is consumed.

Candidate n occurs at `epoch + 120 + 210*n + jitter(n)` real seconds, where
seeded jitter is in `[0,90)`. Thus successive intervals are within 120..300
seconds. A counter-based hash chooses 94.9% faint, 5% bright, 0.1% fireball.
These are presentation choices, not measured astronomical occurrence rates.
The mean rate is approximately 17.14 candidates per real hour while enabled.

Every update computes the latest due slot in constant time. At most one
candidate is considered per frame; older slots count as skipped and an already
expired latest candidate is skipped too. Accepted events retain their scheduled
birth time, so frame rate does not change their age. No backlog is allocated.
UTC changes, pause and time scaling do not affect this monotonic schedule.
Unavailable environments, interiors and world-scope changes invalidate events
and reset the schedule; re-entry starts a new bounded delay. Manual spawn/clear
does not consume, shift or reseed the automatic queue.

Automatic placement uses observer position and seeded tangent directions,
independently of camera heading, FOV, daylight or cloud coverage. Rendering
determines visibility separately. The manual diagnostic placement still uses
camera heading. Track starts are 80..110 physical km; the initial straight-line
trajectory is a diagnostic approximation pending the ablation stage.

`MINIMIUM_METEORS=0` disables automatic births at startup; the enabled flag is
runtime-local and will join the typed property registry in E047-14. Manual
commands remain available. `sky meteors status` reports enabled state, seed,
interval, next deadline, spawned/skipped totals, active peak and current gate.
`sky info` reports actual rendering counts and the altitude gate separately.

Active events share the eight-event cap, preallocated once. Each has eight trail
slots; the frame separately caps 16 cores, 128 trail points and four lights.
Sources outside the view still expire. No fragment or dust allocation is added
by this stage. Numerical compilation/static verification is separate from the
project's gated runtime and visual acceptance.
