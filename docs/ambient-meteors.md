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
camera heading. Track starts are 80..110 physical km; the seeded entry basis is advanced by the physical model described below.

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
## Ablation and photometry (E047-05)

The CPU renderer model supplies six seeded presentation materials: stony, iron,
brittle, porous, heterogeneous and cracked. Densities span 800..7800 kg/m³;
effective ablation heat is 3..8 MJ/kg, heat transfer 0.04..0.16, erosion
multiplier 0..1.2 and visible efficiency 0.015..0.035. These are approximate
profiles, not measured composition inference. Material strength is reserved
for the next fragmentation stage. Preset initial masses are 0.002, 0.5 and
100 kg; seeded entry speeds remain within 11..72 km/s.

The solver uses f64 SI coordinates, exponential atmosphere density with a
7200 m scale height, sphere cross section from current mass/density,
drag-proportional heating and bounded mass loss. Exponential velocity damping
cannot reverse the body. Visible power follows the loss of kinetic energy with
a 60 ms gas cooling response; physical radius falls with the cube root of mass,
while luminous radius follows solid radius and visible power separately.
There is no combustion flame simulation, forced explosion or impact entity.
Gravity, full atmospheric profiles, spectral lines and CFD are outside this
phenomenological model. One of four fixed palettes varies gently with speed;
continuous seeded flicker does not reroll colour each frame.

`Evolution` precomputes at most 512 fixed samples, four 10 ms substeps per
40 ms sample, once per event. Frames interpolate that history, including past
trail positions. Integration stops after optical cooling of exhausted/slowed
material or atmospheric exit, with a 20-second hard presentation cap. A lower
35 km cutoff prevents cosmetic impacts; residual solid mass can survive after
emission and is discarded only as local presentation state. Runtime diagnostics
report material, remaining mass, speed, solid radius and gas emission power.

The same emission supplies source intensity (`power / 4π`) and visible-core
radiance (`intensity / projected luminous area`). Angular filtering supplies
inverse-square dilution for unresolved sources, without an additional distance
division in the shader. The current wake receives 15% of source emission spread
over its total projected geometry area; it does not multiply source power by
trail length. A deposition-driven persistent train is the next trail stage.
HDR exposure and output transfer remain shared. Absolute radiometry and visual
acceptance are not claimed; units here establish a consistent local model.
