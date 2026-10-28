# Performance policy and metric reference

## Target and evidence boundary

The long-term research target is 500 connected players on a Pentium-class 500 MHz server under a specified workload. This is a target to investigate, not measured capacity. No benchmark result or lane-equivalence result is asserted here. Runtime testing and load measurement are deferred until an explicit final-verification instruction.

The current instrumentation can report observations. It does not establish that the prototype is production-ready, performs well on the target hardware, or behaves equivalently across lane counts. Duration percentiles use fixed, source-specific histograms. The estimated percentile is the upper bound of the bucket containing the nearest-rank sample; a sample in the overflow bucket reports the observed maximum. With no duration samples, summary fields are `n/a` (and JSON optional values are `null`).

Histogram boundaries are in microseconds. Server ticks and work phases use `50, 100, 250, 500, 1,000, 2,500, 5,000, 50,000`, then overflow. Client frame intervals and render work use `4,000, 6,000, 8,000, 10,000, 12,000, 14,000, 16,000, 18,000, 20,000, 22,000, 25,000, 28,000, 33,000, 40,000, 50,000`, then overflow; this provides boundaries around 8, 16, 20, and 33 ms frame targets. Load-generator RTT uses `50, 100, 250, 500, 1,000, 2,000, 5,000, 10,000, 20,000, 50,000, 100,000, 250,000, 500,000, 1,000,000, 2,500,000, 5,000,000`, then overflow. Buckets compare the full nanosecond duration against these boundaries; this changes sub-microsecond boundary placement from the earlier truncated RTT histogram. Histogram recording uses fixed-size counters and does not allocate; periodic process sampling may allocate on platforms that read process files.

Static review identifies potential allocation costs in executor batch vectors and shared jobs, copy-on-write world maps, snapshot candidate vectors and player clones, and growing network or renderer buffers. These are candidates for measurement, not measured regressions or savings. Allocation phase attribution follows submitted jobs into executor workers when `allocation-metrics` is enabled; the feature remains off by default.

The source families are `TickDurationStats` (`DurationStats::ticks`) for server tick and phase durations, `FrameDurationStats` (`DurationStats::frames`) for client redraw intervals and render work, and `RttDurationStats` (`DurationStats::rtt`) for load-generator ping/pong latency. Their boundaries are deliberately source-specific; values from the three families should not be compared as though they had equal resolution. The client session's latest interactive ping RTT is a single sample exposed by `net stats`, not a histogram percentile.

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

Executor jobs inherit the submitting thread's allocation phase. Preparing a save snapshot and writing it on the storage worker are attributed to `storage`. Parallel simulation still creates executor dispatch containers and closures; snapshot preparation also allocates a bounded recipient list and executor containers. These are candidates for later measurement, not evidence of allocation-free ticks. Client frame timing and console vertex buffers use fixed or reusable storage; formatting messages can allocate when events occur.

The save request channel has capacity one; the main lane captures copy-on-write shard views, while traversal, sorting and disk writes occur on the worker. The status reports a failed pending save if the completion channel disconnects; it does not prove durability or successful recovery after a crash.

Movement batches retain their buffers across ticks. Core performs copy-on-write of player and cell indexes before dispatch, as in the prior implementation; snapshot references can therefore trigger copies. Worker completion order does not set merge order: the executor restores submission order before the main lane applies results. HashMap iteration is not a cross-run ordering guarantee, and equivalence across lane settings remains unverified.

`status` provides the actual `game_address`, configured `lanes`, active `players`, `ticks`, `save_pending`, `save_last`, `max_players`, `map_fingerprint`, `tick_ms`, `snapshot_ms`, `autosave_seconds`, `protocol_version`, and `save_version`. `lanes` includes the main server lane. `save_pending` is 0 or 1; `save_last` is `never`, `pending`, `ok`, or `error_<message>`. These describe asynchronous persistence; `world save` reports queue acceptance, not disk completion.

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

`net stats` reports `tx`/`rx` application bytes, the latest `rtt` sample, and current nearby-player count. These are client observations and are not a measurement of network-wide TCP overhead. `frame_ms` samples are wall time between redraw callbacks; `cpu_work_ms` samples wall time around renderer update/render work. Both use the frame histogram family.

Frame samples include redraw callbacks where presentation was skipped; those increment `frames` but not `presented`. `frame_ms` and `cpu_work_ms` averages/percentiles are `n/a` before their first sample. The load generator measures RTT from sending a ping until receiving its pong, using monotonic elapsed time; setup pings are excluded from measurement statistics.

## Load-test report v2

The load generator emits text or one JSON object with `schema_version: 2`. It records scenario, OS/architecture, requested and connected clients, generator worker count, elapsed time, pongs/moves/shots/chat/snapshots and rates, errors, rate limits, timeouts, application bytes, and RTT sample/minimum/average/p95/p99/maximum. Metadata includes run ID, rustc version, build profile, scenario ID, optional user-supplied CPU label/clock, generator address/duration/thread mode, and optional authenticated `status`/`metrics show` samples before and after the run. Server sampling requires `--server-data-dir` and occurs only at run boundaries. It does not capture the CPU model or clock automatically.

Field definitions and the prepared scenario manifest are maintained in [scenario report v2](scenarios/report-v2.md). For comparable measurements, record hardware and OS, compiler profile, server lane count, generator thread mode, map, duration, player distribution, and gameplay load. Protocol byte totals exclude TCP/IP headers and retransmissions; RTT percentiles use fixed buckets.

## Deferred measurement matrix

### Initial research budgets

The following are proposed acceptance thresholds for a future, explicitly authorized measurement, not results or claims about the current prototype. They make the research target falsifiable and can be revised with recorded rationale when hardware and workload evidence is available.

- Server tick: p95 at most 50 ms and no missed ticks during the measured interval, with a fixed 20 Hz simulation. Record p99 and maximum as well.
- Process CPU: at most one logical CPU on average over the interval in the single-lane run, calculated as the change in `process_cpu_time_ms` divided by elapsed wall time. Report actual OS thread count separately; cumulative CPU time is not utilization by itself.
- Server memory: peak RSS at most 512 MiB during the interval, including fixed overhead and helper threads. Record the starting RSS and current-player count alongside the peak.
- Game traffic: average at most 8 KiB/s per connected player in each direction, calculated from server application-byte counter deltas divided by elapsed seconds and the mean connected-player count. Report the per-client distribution as well; this budget excludes TCP/IP headers and retransmissions.

For the 500 MHz research case, use a documented Pentium-class 500 MHz machine, record its exact model, memory, OS/kernel, compiler and target configuration, and put the load generator on a separate documented machine. Run the 500-client `clustered` and `dispersed` manifests on `performance.map`, 60 seconds each, server lane 1, generator worker count and all server settings recorded. Adapt their loopback addresses and directories for the [two-host procedure](scenarios/README.md). Record successful handshakes and disconnects, and apply the thresholds only when all 500 clients remain connected throughout the measured interval. Capture server metrics before and after and retain the report and hardware description. The other 2/100/500 and 1/2/auto combinations remain comparison scenarios; none are measurements until run. Suitable historical hardware and a compatible Rust target have not yet been established.

The prepared 18-run matrix is `clustered` and `dispersed` movement × 2/100/500 clients × server lanes 1/2/`auto` × 60 seconds. Run the load generator's worker threads independently from server lanes and record both. It has not been run.

The current movement path uses inline work for one lane and dispatches player movement in batches at 32 or more active players. This threshold and batch size (16 players per batch) are current implementation details, not a claim that parallel execution improves throughput.

The TCP sender rotates its starting client, limits each client to 16 KiB per poll and limits the iteration to 256 KiB; the receiver limits each client to 1024 bytes and 32 frames. Slow-consumer, partial-write, EOF and fairness behavior requires the deferred network scenarios; these budgets alone do not establish fairness under load.

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

Each client output is capped at 64 KiB. Frame timestamp metadata has at most one entry per queued application frame, with a separate cap of 256 entries per client (admission fails at either the byte or frame limit). During a poll, the server writes at most 16 KiB for one client and 256 KiB total. It rotates the first client considered by one position on each poll, so the first client does not always consume the shared budget. Reads are capped at 1 KiB and 32 decoded messages per client per poll. Pending world input plus live client count is capped at 4096, reserving room for one Leave per live client. The admin dispatcher queue and terminal adapter queue each hold at most 64 commands; at most eight dispatch per tick. Admin has at most eight connections, four accepts per poll, 512-byte lines, two-second request and reply deadlines, 64 KiB response cap, and 4 KiB writes per client per poll. The interactive client's `ClientSession` retains at most 64 KiB for each send/receive buffer and reads at most 16 KiB per poll. Endpoint reads are capped at 256 bytes and shutdown result reads at 4096. Linux process text samples are capped at 64 KiB per file.
