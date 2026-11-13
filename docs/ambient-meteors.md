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

The same emission supplies source intensity (`power / 4π`) and, in the GPU adapter, visible-core
radiance (`intensity / projected luminous area`). Angular filtering supplies
inverse-square dilution for unresolved sources, without an additional distance
division in the shader. The current wake receives 15% of source emission spread
over its total projected geometry area; it does not multiply source power by
trail length. A deposition-driven persistent train is the next trail stage.
HDR exposure and output transfer remain shared. Absolute radiometry and visual
acceptance are not claimed; units here establish a consistent local model.


## Fragmentation and erosion (E047-06)

`fragmentation::System` owns up to four solid bodies and four grain packets per
event. Dynamic pressure `q = 0.5*rho*v�` crossing the seeded material strength
splits a surviving body; height alone never triggers breakup. At most two
breakup generations and three daughter slots bound the cascade. Raised
post-failure thresholds prevent repeated triggering at the same pressure.
The faint 0.002 kg profile is below the 0.02 kg breakup eligibility floor.

Each split transfers 20..45% of the remaining parent mass to a daughter at the
same release position, with seeded 5..25 m/s transverse dispersion. A tiny
forward-speed reduction pays the dispersion kinetic-energy budget. Gas power
is shared by the same mass fraction, so inherited glow is not duplicated.
Each body subsequently has independent drag, ablation, phase, position, speed
and emission; increased surface area can create one or several energy-driven
flares, including late failure. Strong material can ablate without breakup or
an energetic flare. There is no imposed explosion pulse or physical implosion.

Ablation and eroded dust are separate mass flows. Eroded mass enters a bounded
accumulator and may transfer once into a grain packet. Packets represent many
fine grains using an effective area profile; the packet counter is not a
meteoroid count or extra mass. Unrepresented dust remains in the mass ledger.
Its carried kinetic energy is excluded from parent light and may be deposited
later by a represented packet. Final solid + packet dust + unrepresented dust
+ ablated mass accounts for the initial mass; status prints the residual.

The bounded process log carries Ablation, Erosion, Breakup, Flare, DustRelease,
Extinction and DarkFlightEnd separately from rendering. Flare records identify
rapid increases of computed power, not added energy. The last two of 64 entries
are reserved for extinction/disposal; omitted entries have a counter. DarkFlightEnd
means local disposal without impact simulation. Diagnostic status shows recent
process entries, mass ledger, packet count and omitted core submissions.

All eight event histories are preallocated at runtime creation and returned to
a pool on expiry, clear, failed initialization or world invalidation. Each has
512 snapshots of eight bodies: at most 3,407,872 bytes of sample storage with
the current Body layout, plus bounded metadata. Spawn performs no allocation
for histories. Samples retain release times and positions so new branches
never interpolate from the planet origin or appear before their birth.

Solid samples compete by received source power for the existing 16-core frame
cap; grain samples only fill remaining slots. At most 32 grain packets can be
active in this implementation, below the 64-packet ceiling. Existing eight
trail points per retained core give separate short wakes; dropped cores reclaim
their trails. Rendering uses the same instanced pipeline and cloud resolve.
`sky meteors spawn fragmenting` selects a brittle, steeper diagnostic entry;
`fireball` retains the seeded material distribution. These presets do not
bypass pressure thresholds. Visual separation and temporal acceptance remain
part of the explicitly gated final verification.
