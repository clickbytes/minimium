# minimium-telemetry

## Purpose / Non-goals

Owns duration metrics, process sampling, the asynchronous diagnostics backend and optional allocation instrumentation. It does not own domain state or decide gameplay behavior.

## Public API

Duration, process, bounded diagnostic logging and allocation facilities are exposed from `src/lib.rs`; allocation instrumentation is feature-gated.

## Dependencies

Depends on `minimium-diagnostics` for its domain-neutral event contract. Domain code depends on the diagnostics contract, never on this infrastructure package.

## Data flow

Runtime and application call sites submit diagnostic events through a nonblocking bounded sink. A single writer owns each run directory and writes bounded JSON Lines records into per-domain files. The JSONL writer budgets the fully escaped record before allocating field fragments and reserves space for the mandatory `truncated` marker. Telemetry also exposes process metrics and allocation instrumentation.

Each run directory has a generated run ID and stores the optional parent run ID
for a local child server. UTC date plus an atomic daily sequence allocates a
unique directory. Completed runs older than seven days are removed; completed
directories are also evicted oldest-first above the 2 GiB total cap. Active
directories are protected by a lock. Each domain log rotates at 16 MiB and
retains two prior segments. Use the application-created `RunDirectory` and
`DiagnosticsRuntime`; domains receive only the diagnostics contract and cannot
choose paths or retention policy.

## Ownership and execution

The process owns `DiagnosticsRuntime`. Its bounded `shutdown_until` closes the
process sink, rejects later submissions and waits for the writer to drain queued
records until the caller's deadline. The writer can finish even if another
worker still holds a cloned logger.
Server lifecycle supplies the absolute process shutdown deadline, reserving its
final two seconds for this flush. Timeout reports an error and detaches the
writer; it does not claim that an in-progress OS write was cancelled. Ordinary
drop applies a two-second fallback deadline.

Metric stores are owned by the collecting runtime; allocation accounting uses its documented thread-local mechanism.

## Invariants and errors

Keep queues, records, rotation and retained run data bounded. File errors are available to the owning application and use stderr only as a process-boundary fallback. Optional allocation instrumentation remains disabled unless explicitly enabled.

## Commands

No domain commands.

## Change guide

Change measurement representation here; avoid adding telemetry dependencies to core world algorithms.

## Verification

Compilation and static validation are deferred to the workspace final verification phase.
