# ADR 0016  Committed simulation time

Accepted 2026-11-19. Supersedes the time model in ADRs 0006, 0013 and 0015.

## Decision

The server owns one committed tick and UTC for all worlds. Each completed tick advances both by 50 ms. Requested scale converts monotonic nanoseconds into integer demand with a retained fractional remainder. Each host pass starts steps within a 4 ms budget. At ordinary rates the batch holds eight steps. Fast-forward raises the cap to the requested multiplier rounded up, capped at 1024, so inexpensive steps can use the available work budget. Excess demand is counted and discarded. Host services, journal acknowledgements, administration and connection deadlines continue independently.

Water normally advances every two ticks. An outstanding authoritative job holds its target boundary. Results must match the world and input revisions. An irreversible transaction completes before pause acknowledgement or snapshot capture. Resume discards old demand and earlier action revisions. The gameplay module stages keep their fixed ordering.

Explicit dates and entry into real-time mode change the calendar epoch. Offline advance changes the calendar on restart. Object age always follows the saved tick. The UTC range boundary pauses the game and exposes a recoverable clock fault.

Protocol 22 carries committed samples. The client interpolates them once per frame and holds at the latest evidence when packets stop. Continuous movement is refreshed on a real deadline. Snapshots identify the tick where a sequence took effect. World animation and audio advance on the presentation timeline. Night eligibility binds position and context to a conservative UTC interval from the solar model. Weather selects valid detailed or local current-date data for clouds, lighting, ground and fog.

Save 12 binds the clock to the world snapshot and durable journal boundary. Prior formats require a separate data directory. An incomplete shutdown reports failure and skips final snapshot publication. Backups and restores operate on a matching set of files.

## Acceptance

Static checks cover the workspace, headless and audio feature configurations, owner boundaries and file limits. Final runtime verification covers two clients at 1x and 0.1x, pause and resume during outstanding work, high requested rates under load, weather at identical dates, twilight, disconnects, restart and UTC exhaustion. GPU and listening measurements require that phase.
