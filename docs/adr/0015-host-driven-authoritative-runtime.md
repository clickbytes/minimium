# ADR 0015: Host-driven authoritative runtime

**Time model amendment.** ADR 0016 supersedes this decision's clock progression, effect lifetime, persistence and host tick cadence rules. The original decision below records the earlier design. [ADR 0016](0016-committed-simulation-time.md)

Status: Accepted for E049-S13. Runtime acceptance remains deferred to Q03.

The server process and an embedded host use the same `ServerRuntime` owner,
constructor, command queue, validated built-in schedule and `step`. There is no
second simulation executor. `run` and `run_with_config` are process adapters:
they prepare file/content/transport services, randomness, diagnostics, console
input and execution pools, then drive the public owner API.

`RuntimeModules` prepares the component catalog before storage load. Construction
takes prepared worlds/restores, path-free settings, explicit services, prepared
`RuntimeExecution` pools and host clock/session-nonce inputs. The constructor
starts no TCP listener, filesystem operation, child process or console thread.
It validates raw restore identities and bounds, component payloads and journal
checkpoints, and acquires the snapshot reservations for injected entities before
returning the owner. The application remains responsible for selecting content.

`submit` accepts a bounded parsed administration intent and returns a submission
ID or typed rejection. The existing 64-entry queue is shared with console/admin
adapters. A fixed tick executes at most eight commands; execution results are
returned separately by `step`. Host possession grants administration authority;
remote principals and gameplay inputs continue through the supplied typed
transport port and the same owner-side admission/action rules. Queries borrow
the owner immutably and return bounded status/world/player snapshots. They do not
advance time or mutate state. Stop cancels remaining submissions with explicit
results; fatal step errors stop further admission and retain cancellation results
for a subsequent step or shutdown.

Each step receives one monotonic instant and a UTC value or typed UTC failure.
Owner deadlines, cooldowns and snapshot times use that instant. UTC correction,
save failures, fixed simulation dt, one-tick-per-iteration and missed-tick policy
retain their existing semantics. Wall-clock reads remain in the explicit process
clock adapter. Real elapsed measurements and OS worker-drain deadlines are
instrumentation/resource concerns, not simulation time. Shutdown freezes the
owner at the host's final supplied time and drains services by the supplied
absolute deadline. It reports incomplete work as failure.

`MemoryStorageBackend` is real volatile persistence: snapshot encoding/loading
uses the same format routines as files; excavation and impact commits use the
same structural validation and transition rules. Acceptance is not completion.
Polling commits a pending operation to the shared memory image before returning
its acknowledgement. Clones retain the image, but only one opened service lease
may exist; load/reopen requires release of all ports. Memory acknowledgement
does not promise survival of process termination. Retained snapshots/journals,
body counts and pending queues have explicit bounds; transient validation copies
are also bounded by those limits. Format versions and encoded bytes are unchanged.

The compileable `embedded_host` example supplies local-only services, drives
the owner, submits a save, queries status, shuts down and reloads the memory
snapshot. Compilation proves API usability; process/embedded parity, resource
exhaustion, cancellation and persistence recovery still require Q03 execution.
