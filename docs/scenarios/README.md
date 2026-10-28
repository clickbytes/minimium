# Repeatable load scenarios

Each TOML file describes one server and generator invocation. Start the server
with its `server_command`, then run the load generator with `loadtest_command`.
The server game port and data directory are unique per manifest. These runs
are intended for the final verification phase and are not evidence of
performance until executed and reported with the hardware and build profile.

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
