# Performance policy and metric reference

## Target and evidence boundary

The long-term research target is 500 connected players on a Pentium-class 500 MHz server under a specified workload. This is a target to investigate, not measured capacity. No benchmark result or lane-equivalence result is asserted here. Runtime testing and load measurement are deferred until an explicit final-verification instruction.

The current instrumentation can report observations. It does not establish that the prototype is production-ready, performs well on the target hardware, or behaves equivalently across lane counts. `p95` and `p99` duration values are estimates from fixed histogram buckets; samples beyond the last bucket report the observed maximum.

## Server `metrics show`

The command reports space-separated `key=value` fields. Counters are cumulative since server start unless identified as last/current/peak. Main groups and exact field names:

- **Simulation cadence:** `ticks`, `missed_ticks`, `tick_last_us`, `tick_avg_us`, `tick_p95_us`, `tick_p99_us`, plus `snapshots`, `missed_snapshots`.
- **Loop phases:** `loop_avg_us/p95_us/p99_us`, `network_avg_us/p95_us/p99_us`, `input_avg_us/p95_us/p99_us`, `simulation_avg_us/p95_us/p99_us`, `snapshot_avg_us/p95_us/p99_us`, `encoding_avg_us/p95_us/p99_us`, and `input_wait_avg_us/p95_us/p99_us`.
- **Traffic and outcomes:** `bytes_in`, `bytes_out`, `decoded_frames`, `queued_frames`, `dropped_messages`, `dropped_snapshots`, `rejected_inputs`, `idle_timeouts`, `handshake_timeouts`, `shots_fired`, `shots_hit`, and `player_kills`. Byte counters count application game bytes handled for admitted game clients; they exclude admin IPC and one-shot rejection writes. `queued_frames` counts frames admitted to output queues, not frames acknowledged by the remote peer.
- **Queue and output bounds:** `max_pending_inputs`, `max_client_input_bytes`, `max_client_output_bytes`, and `oldest_output_age_us`.
- **Spatial query work:** `collision_queries`, `collision_cells`, `collision_cells_per_query`, `collision_max_cells_per_query`, `collision_aabb_candidates`, `collision_aabb_candidates_per_query`, `collision_max_aabb_candidates_per_query`, `collision_triangle_candidates`, `collision_triangle_candidates_per_query`, `collision_max_triangle_candidates_per_query`, and `collision_full_scan_queries`.
- **Process samples:** `memory_samples`, `memory_rss_bytes`, `memory_peak_rss_bytes`, `memory_private_bytes`, `memory_peak_private_bytes`, `memory_rss_average_per_player_bytes`, `process_cpu_time_ms`, and `process_threads`. Memory and process values refresh at most once per second. Process CPU time is cumulative CPU time across the process's OS threads, not CPU utilization; unavailable platform values are `n/a`. RSS-per-player divides total process RSS, including fixed overhead, by current active players.
- **Per-client traffic:** `client_traffic=id:bytes_in:bytes_out:decoded_frames:queued_frames:encoding_us:connected_ms` appends active authenticated clients' values.
- **Optional allocation counters:** with the server's `allocation-metrics` feature enabled, `alloc_<phase>_allocations`, `alloc_<phase>_reallocations`, and `alloc_<phase>_bytes` are included for `other`, `network`, `commands`, `input`, `simulation`, `snapshot`, and `storage`, followed by `allocation_metrics=enabled`. Otherwise counters are absent and `allocation_metrics=disabled`. Counts cover successful allocation/reallocation requests and requested sizes; they are cumulative and are not live or retained-memory totals.

`status` provides the configured `lanes`, active `players`, `ticks`, `save_pending`, `save_last`, `max_players`, `map_fingerprint`, `tick_ms`, `snapshot_ms`, `autosave_seconds`, `protocol_version`, and `save_version`. `lanes` includes the main server lane. `save_pending` is 0 or 1; `save_last` is `never`, `pending`, `ok`, or `error_<message>`. These describe asynchronous persistence; `world save` reports queue acceptance, not disk completion.

On Linux, process memory uses `/proc`; on Windows it reports working-set and private committed bytes. Process CPU time is cumulative OS-reported user plus kernel time. Sampling is cached for one second; it is not a per-tick CPU measurement.

## Client frame and network metrics

The in-window `fps show` command reports:

- `frames` and `presented` sample/presentation counts;
- `presented_FPS_avg`;
- `frame_ms(avg/p95/p99)`, wall time between redraw callbacks;
- `cpu_work_ms(avg/p95/p99)`, wall-clock elapsed around renderer update and render work, not process CPU time;
- `draw_calls_per_presented_frame`;
- `process_cpu_time_ms`, cumulative CPU time across client process threads, sampled/cached about once per second;
- `engine_GPU_estimate`, a renderer-side estimate from tracked resources and surface dimensions, not a driver-reported total.

`net stats` reports `tx`/`rx` application bytes, the latest `rtt` sample, and current nearby-player count. These are client observations and are not a measurement of network-wide TCP overhead.

Frame samples include redraw callbacks where presentation was skipped; those increment `frames` but not `presented`. The percentile values share the fixed-bucket approximation described above.

## Load-test report v2

The load generator emits text or one JSON object with `schema_version: 2`. It records scenario, OS/architecture, requested and connected clients, generator worker count, elapsed time, pongs/moves/shots/chat/snapshots and rates, errors, rate limits, timeouts, application bytes, and RTT sample/minimum/average/p95/p99/maximum. Metadata includes run ID, rustc version, build profile, scenario ID, optional user-supplied CPU label/clock, generator address/duration/thread mode, and optional authenticated `status`/`metrics show` samples before and after the run. Server sampling requires `--server-data-dir` and occurs only at run boundaries. It does not capture the CPU model or clock automatically.

Field definitions and the prepared scenario manifest are maintained in [scenario report v2](scenarios/report-v2.md). For comparable measurements, record hardware and OS, compiler profile, server lane count, generator thread mode, map, duration, player distribution, and gameplay load. Protocol byte totals exclude TCP/IP headers and retransmissions; RTT percentiles use fixed buckets.

## Deferred measurement matrix

The prepared 18-run matrix is `clustered` and `dispersed` movement × 2/100/500 clients × server lanes 1/2/`auto` × 60 seconds. Run the load generator's worker threads independently from server lanes and record both. It has not been run.

The current movement path uses inline work for one lane and dispatches player movement in batches at 32 or more active players. This threshold and batch size (16 players per batch) are current implementation details, not a claim that parallel execution improves throughput.

Final verification must also compare authoritative outcomes under lanes 1, 2, and `auto` with at least 32 active players, exercise the demo route and snapshot full/delta/removal behavior, and record high-percentile latency alongside averages. The 500 MHz target remains research work until measurements on specified hardware and repeatable scenarios support a conclusion.

## Measurement rules

- Keep all queues and memory growth bounded; include metadata overhead when reviewing queue limits.
- Reuse buffers in tick, network, and replication paths.
- Measure phase costs, synchronization, process memory, and tail latency before tuning.
- Compare lane modes under the same map, player distribution, build profile, and workload.
- Keep rendering and visual assets out of the headless server.

## Counter update points

`bytes_in` increments after successful socket reads; `decoded_frames` after a complete validated client frame. `queued_frames` increments only after an encoded frame is admitted to the bounded output queue. `bytes_out` counts actual successful socket writes, including progress before WouldBlock or a fatal error. A fatal write disconnects the client in the same poll. These are application frames, not TCP packets.

`encoding_*` measures encode attempts; `client_traffic.encoding_us` sums encodes specific to that client. Chat is encoded once for all recipients and contributes only to the shared encoding statistic. `connected_ms` lets a report derive each client's bytes/frames per second (counter divided by connected_ms/1000); compare before/after deltas for interval rates. Disconnected clients' totals are logged on removal and retained in aggregate counters.

`input_wait_*` spans decode to application in a simulation tick, excluding server-generated Leave events. `input_*` measures execution of that batch, and `simulation_*` measures the movement/collision phase. `oldest_output_age_us` is the age of the oldest still-unsent admitted frame after flushing, zero for an empty queue. Timestamp entries are removed only as their bytes are fully written. `tick_*` covers commands, input application and simulation; `loop_*` also includes the current poll and any scheduled snapshot preparation. Network and snapshot work may occur outside a tick.

Each client output is capped at 64 KiB. Frame timestamp metadata has at most one entry per queued application frame, with a separate cap of 256 entries per client (admission fails at either the byte or frame limit). Pending world input plus live client count is capped at 4096, reserving room for one Leave per live client. The admin dispatcher queue and terminal adapter queue each hold at most 64 commands; at most eight dispatch per tick. Admin has at most eight connections, four accepts per poll, 512-byte lines, two-second request and reply deadlines, and a 64 KiB response cap. Endpoint reads are capped at 256 bytes and shutdown result reads at 4096. Linux process text samples are capped at 64 KiB per file.
