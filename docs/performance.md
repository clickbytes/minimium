# Performance Policy

## Goal

The long-term research target is 500 connected players on a Pentium-class 500 MHz server under a defined gameplay scenario. This is an aspiration to measure, not a current performance claim.

## Required measurements

- Server tick and phase durations; missed ticks.
- Process memory and memory per connected player.
- Allocations in critical paths.
- Inbound and outbound bytes and packets per player.
- Queue depth and dropped or coalesced state updates.
- Client frame time, draw submissions, GPU memory and asset residency.

The current server `metrics show` command reports tick and active-loop timing, p95/p99 tick duration, network, input-application, simulation and snapshot preparation p95 buckets, missed tick and snapshot counts, traffic bytes, rejected inputs, dropped snapshots, and high-water marks for pending inputs and per-client buffers. Percentiles are bucket estimates; samples above the final bucket report the observed maximum. Process memory, allocation counts, per-player normalization, hardware metadata and repeatable scenario reports still need implementation.

Every report records hardware, OS, compiler profile, lane count, tick rate, player distribution, AI and physics load. Compare 1, 2 and automatic execution lanes. Report high-percentile latency in addition to averages.

## Implementation rules

- Bound all queues and memory growth.
- Reuse buffers in the tick, network and replication paths.
- Use spatial locality to avoid all-pairs entity work.
- Reduce update frequency for distant or inactive simulation when game rules allow it.
- Sleep or block while idle; do not spin.
- Measure before parallelizing; report synchronization cost.
- Keep rendering and visual assets out of the headless server.

Manual world saves use a bounded request channel and a dedicated storage thread so player traversal, sorting, formatting, sync and atomic replacement do not run in the simulation loop. Capture updates active players and copies 256 shard handles; copy-on-write may copy one shard if a player joins or changes name while a save is in flight. Measure this behavior and shard occupancy before raising the stored-player limit substantially.

The load-test executable will reproduce saved scenarios and emit machine-readable results. Performance changes are compared using the same scenario and build profile.
