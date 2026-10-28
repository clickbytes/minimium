# Repeatable load scenarios

Each TOML file describes one server and generator invocation. Start the server
with its `server_command`, then run the load generator with `loadtest_command`.
The server game port and data directory are unique per manifest. These runs
are intended for the final verification phase and are not evidence of
performance until executed and reported with the hardware and build profile.

The listed data directories are examples, not clean-run directories. For every
repetition use a new, empty directory incorporating a unique run ID. Do not
delete or overwrite earlier saves: persistent profiles accumulate between runs
and change memory and startup costs. Record the actual directory and port.

## Separate server and generator hosts

The manifests use loopback for a single-machine baseline. For two hosts,
replace the server's `--bind 127.0.0.1` with its LAN address and the generator's
`--address 127.0.0.1:PORT` with that address and port. Use an isolated network;
game traffic has no authentication or encryption. Keep administration on the
server's loopback interface. Do not pass `--server-data-dir` to the remote
generator: it reads a local endpoint and connects to local loopback.

On the server host, capture `minimiumctl --data-dir RUN_DIR status` and
`metrics show` before and after the workload as separate artifacts. Label them
with the generator's scenario ID and run ID; record UTC before and after each
request, hardware, OS, build, and the measured clock offset and uncertainty
between machines. These samples are not automatically merged into generator
JSON. The generator has no absolute UTC start/end marker, so even synchronized
clocks do not establish exact shared 60-second boundaries. Bracket the run and
report the uncertainty; do not assert an exact server delta. The built-in
local `measurement_start` precedes the shared start and `after` follows worker
joins, so their difference can include setup traffic and extra time as well.

All cases use `games/sample/maps/performance.map`, last 60 seconds, and pin the
generator to one worker thread. `server_lanes` controls only server execution
lanes. The generator waits for a valid server `Welcome`, sends `SetName`, and
waits for its acknowledgement before the measurement window. Each worker
services and sends bounded setup heartbeats to already-ready sockets while it
prepares later clients and waits for the shared start. Connected-at-start is
the last live population observed during this drain, not a guarantee that every
socket was alive at one exact instant. Once every worker is ready, it captures
the server's measurement-start sample and releases all
workers against one shared monotonic start time. Setup bytes are reported
separately from workload-window bytes; clients that disconnect before release
are counted separately from failed handshakes and in-window disconnects.
Clustered bots alternate opposite
inputs every ten 50 ms movement ticks,
which keeps their authoritative paths close to the spawn without teleporting.
Dispersed bots use a unique normalized radial direction at angle
`2π * client_id / requested_client_count` and keep it for the run. Each
movement frame is encoded once before that worker's timed loop. Chat offsets
are measured from the common workload start, so worker setup time cannot
compress the stagger into a burst. The map is a 4096 by 4096 flat solid floor
centered at the origin.
