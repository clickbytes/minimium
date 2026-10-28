# Epic 002 — verification ledger

Runtime phase executed on 2026-10-28 after the user's completion instruction. Environment: Windows 10 IoT Enterprise LTSC build 19044, x86_64-pc-windows-msvc, rustc 1.99.0 (b940084d7 2026-09-28), 12th Gen Intel Core i5-12600K, 10 cores / 16 logical processors, 128 GiB visible memory. Release workloads used binaries built at integration head `df83342f677f970c82a6c5ab64c55a95692622dc`. Main was `53314514dac1456744af909aa4a1ea910458d111` and an ancestor of that head.

| Gate | Command | Result |
| --- | --- | --- |
| Format | `cargo fmt --all -- --check` | PASS at final working tree |
| Types | `cargo check --workspace --locked --offline` | PASS at final working tree |
| Static lint | `cargo clippy --workspace --locked --offline` | PASS with existing warnings: `large_enum_variant` for `ServerMessage` and `SessionEvent`, and `manual_div_ceil` in telemetry |
| Release build | `cargo build --workspace --release --locked --offline` | PASS at final working tree |
| Allocation feature build | `cargo build -p minimium-server --features allocation-metrics --locked --offline` | PASS at final working tree |
| Architecture | `python scripts/check_architecture.py` | PASS, workspace graph only |
| File policy | `python scripts/check_file_lengths.py` | PASS, 104 source files, maximum 500 physical lines |
| Deterministic simulation | `cargo test -p minimium-server --locked --offline` | PASS, 1 test: same 32 fixed player inputs for 40 fixed steps gave identical bitwise positions and velocities at lanes 1, 2 and host `auto` |
| Workspace tests | `cargo test --workspace --locked --offline` | PASS, 1 unit test; all other unit and doc-test targets contain zero tests |
| Integration diff | `git diff --check origin/main` | PASS at final working tree |

The 18 release load runs used `games/sample/maps/performance.map`, 60 seconds each, generator lane 1, server lanes 1/2/auto. Both `clustered` and `dispersed` completed at 2, 100 and 500 requested clients. All 18 reports were schema v2, recorded all requested clients connected at start, had `errors=0`, exit code 0, and contained all three server samples. The reports counted 2,167,200 received snapshots total; each recorded RTT p99 of 20 ms on this host. A raw scan found no `admin_token`, `nonce` or `server_token` strings. Raw reports and process logs remain under `D:\Temp\minimium-epic002-matrix\20261028T160924Z-151e92bd`.

Additional runtime checks passed: two independent console profiles connected to one server, moved, exchanged chat, pinged and completed a hitscan; a duplicate active profile was rejected. A queued save survived a clean restart and restored the profile name. A v2-compatible snapshot derived from a real v3 save loaded with legacy health defaulted to 100, and the next save wrote v3. The local owner started a child server, a second console attached, the attached client's exit left the server alive, and the owner exit stopped its child. A competing server could not acquire an already locked data directory. A raw fragmented v8 header with an incomplete Identify payload was closed; two following clients then connected without errors. Per-run evidence is in `D:\Temp\minimium-epic002-clients`, `D:\Temp\minimium-epic002-v2`, `D:\Temp\minimium-epic002-ownership`, `D:\Temp\minimium-epic002-lock` and `D:\Temp\minimium-epic002-partial`.

Connector smoke checks passed: while DNS for a reserved `.invalid` hostname remained pending, the console still processed `help`; the attempt reported its 10-second timeout. A second console cancelled a connection attempt to a TEST-NET address, reported that cancelled cleanup was still pending, processed `help`, then reconnected to a local server after cleanup and received a `Pong`. A third console timed out on `.invalid`, was held busy while cleanup finished, then reconnected to a local server and received `Pong` at 31 ms. Logs are in `D:\Temp\minimium-epic002-connector`, `D:\Temp\minimium-epic002-reconnect` and `D:\Temp\minimium-epic002-timeout-reconnect`.

Forced storage and recovery checks passed: an injected `world.snapshot.tmp` directory made a queued save report `save_last=error_Access_is_denied_(os_error_5)`. After first writing a valid snapshot, the same failure left its SHA-256 unchanged; a forced process exit followed by restart loaded that prior v3 snapshot. A controlled `stop` during the injected failure surfaced `server shutdown failed: Access_denied` as expected. The isolated data directory and logs are in `D:\Temp\minimium-epic002-crash-recovery` and `D:\Temp\minimium-epic002-storage-failure-run2`.

Admin request bounds passed: an incomplete authentication line was closed after 2.4 seconds, a 513-byte command line received `ERR admin request line exceeds size limit`, and the server then stopped cleanly. The token was read from the isolated endpoint file only by the harness and was not included in output. Evidence is in `D:\Temp\minimium-epic002-admin-limits`.

Backpressure attempt: 64 raw clients used a 4 KiB receive-buffer request, never read snapshots, and sent periodic pings. Runs of 30, 45, and 150 seconds recorded `dropped_snapshots=0`; the longest run with maximum-length test names recorded `max_client_output_bytes=748`. These runs did not fill the OS or server queue and are inconclusive for sustained backpressure; the longest-run evidence is in `D:\Temp\minimium-epic002-backpressure-run3`.

The demo-map collision route passed: after approaching from beyond the stair end, the raw client climbed each step to `y=4.0`, crossed the upper deck, bridge and far platform, stopped at a vertical triangle near `z=-2.5`, then climbed the walkable triangle ramp to about `y=0.8`. Position samples are in `D:\Temp\minimium-epic002-geometry\geometry.csv`.

The desktop demo opened a window whose title showed a live FPS value and 100 HP. Windows Graphics Capture timed out twice, so the 3D scene, console, resize, prediction and interpolation did not receive visual verification. A later Computer Use state request was stopped by the physical Escape key before a screenshot was returned. The displayed FPS is not a benchmark.

An exploratory live lane comparison sampled input and world state at unsynchronized times and observed position differences up to 2.3 units. It did not align workload start and input tick, so it is inconclusive about live network lane equivalence. The controlled fixed-input simulation test passed, but exact live admission/input-tick equivalence remains open.

Still unverified: desktop visual behavior; zero-byte writes; sustained backpressure; slow admin response I/O; exact network lane equivalence; and the Pentium-class 500 MHz target. The 500-client runs establish only that the prepared workloads completed on this Windows host. They do not demonstrate the 500 MHz capacity target.

Clippy's existing `large_enum_variant` warnings for `ServerMessage` and `SessionEvent`, and `manual_div_ceil` in telemetry, remain; boxing or code churn was not added to silence them. The environment reports failed incremental hardlinks and falls back to copying cache files; Git has reported missing commit-graph files. All listed workspace gates now pass at this working tree, but runtime scenarios listed above remain open. PR #31 and issues #30/#14 remain open until the remaining runtime verification and integration review are recorded.
