# E050-18 / AR17: execution-host decision

**Decision: `DEFERRED — limitation consciously retained`.** At reviewed HEAD
`1972371cc67f1a39175d4ec2d0324b45c7a94075`, no documented production consumer
requires a second implementation of the server execution resources or a fully
inline `ServerRuntime`. Keep the existing concrete `Executor` and
`SectorGenerationPool`. This decision records scope; it does not fix AR17 or add
an execution port.

## Evidence and current consumers

`RuntimeExecution` in `crates/server-runtime/src/hosting.rs` owns the concrete
`executor::Executor`, `sector_streaming::SectorGenerationPool`, and optional
console receiver. `RuntimeExecution::new(lanes, sector_workers)` validates
1–64 lanes and 1–16 sector workers, creates both pools, and does not expose
injection of either resource. The process adapter in
`crates/server-runtime/src/lib.rs::run_with_config` calls this constructor and
moves the result to `ServerRuntime::new`. `ServerRuntime::new` in
`crates/server-runtime/src/runtime.rs` takes and stores these prepared resources;
it does not itself create them or start their threads. This distinction is
intentional in ADR 0015 and the runtime README.

The reusable executor serves two production callers:

| Work | Caller and result path |
|---|---|
| Simulation player batches | `crates/server-runtime/src/simulation.rs::step_ready_players` calls `Executor::map`; `crates/server-runtime/src/runtime/tick.rs` supplies the runtime executor. `step_players` is a test-only helper. Each call waits for all batch results, restores input order, and returns them to the owner lane before the tick continues. |
| Remote-player snapshots | `crates/server-runtime/src/replication.rs::replicate_snapshots` calls `Executor::map` for per-client snapshots. `crates/server-runtime/src/runtime/tick.rs` invokes it on the owner lane; it collects results synchronously before queuing messages. |

Both the process adapter and embedded examples use the same
`RuntimeExecution::new` implementation. `crates/server-runtime/examples/embedded_host.rs`
and the other embedded examples use `(1, 1)`; they demonstrate owner API
composition and do not demonstrate a different scheduler, worker service, or
product requirement for pool substitution. The single construction callsites
are the process adapter and these examples; tests construct `Executor` directly
only for executor/simulation coverage.

The sector pool has a separate domain-specific consumer path. In
`crates/server-runtime/src/sector_streaming/workers.rs`, generation jobs carry a
`SectorTicket`, map, manifest, and terrain inputs. A shared state queue bounds
pending + completed + active generation jobs to 64; submission is capped at 64
per update and the owner installs at most eight completed generation results
per tick. The owner lane installs only still-pinned tickets, and stale or
cancelled ticket results are discarded. `prepare` accepts structural tasks only
when queued + active preparation work is below eight. It shares the generation
workers and thread budget. Generation completion/errors are retained in the
pool's `finished` queue until the owner drains them; task-specific structural
preparation closures own their completion channel.

`SectorGenerationPool` also owns `CollisionIndexWorker` in
`crates/server-runtime/src/sector_streaming/collision.rs`. Collision work has one
active job and pending/completed `BTreeMap`s keyed by world instance, so newer
work for one world replaces its pending entry. There is no separate numeric
global queue cap; the maps are bounded by represented world instances and each
job's sector data. The owner polls results, verifies that the world still exists
and that its collision source revision matches the result, then publishes the
index and ticket set. These are not interchangeable with `Executor::map`
semantics.

## Resource, failure, and shutdown contract

`Executor::new` in `crates/server-runtime/src/executor.rs` counts the owner lane
as one lane. One lane constructs no executor worker threads and `map` runs inline
on its caller. More lanes create `lanes - 1` persistent workers with a bounded
sync channel of `max(2 * worker_count, 1)` jobs. `map` is synchronous: it submits
batch closures, executes one batch on the caller, blocks collecting worker
results, restores input order, and returns the completed vector. It has no
per-job cancellation handle, completion polling API, or typed worker-failure
result. A worker panic can disconnect the result channel or make later
`expect`/`send` paths panic; it is not converted into a recoverable task error.
There is no worker replacement.

One-lane execution applies only to this map executor. `RuntimeExecution::new(1,
1)` still creates the sector-generation worker and collision-index worker.
Therefore it is not a fully inline or thread-free runtime setup.

Generation cancellation is ticket-based, not interruption of in-flight work.
`discard_cancelled` removes retired queued/completed tickets; in-flight generation
continues and the owner rejects stale results before install. A worker that exits
while holding a generation ticket is noticed by `collect_worker_exits`; the active
ticket is failed and the worker is not replaced. If no generation workers remain,
`update` fails pending and newly dequeued tickets. Panics in separately accepted
preparation closures are caught by the worker loop. `WorkSubmission::submit` in
`crates/server-runtime/src/runtime/excavation/ports.rs` also catches its edit-job
panic, sends a typed capacity rejection through its one-entry result channel, and
the outer worker catch prevents a panic escaping the pool. Collision publication
requires a live world and matching collision-source revision; collision worker
failure is surfaced through polling or shutdown diagnostics, without replacement.

`lifecycle::shutdown` in `crates/server-runtime/src/lifecycle.rs` passes one
absolute deadline to the sector pool and executor after stopping admission. The
sector pool stops admission, clears pending/preparation/completed queues, lets
active work run until the deadline, and reports workers still unfinished. The
executor drops its sender and polls worker completion until that deadline,
reporting unfinished worker IDs; dropping remaining `JoinHandle`s does not stop
their OS threads. The collision worker also receives the deadline and reports
unfinished work. The final shutdown result records unfinished work as an error.
The deadline bounds the drain wait; it does not preempt an executing closure or
bound a synchronous `Executor::map` already occupying the runtime caller. The
map must return before owner-lane shutdown can start.

## Requirements search

Repository review covered `docs/architecture.md`, `docs/architecture-execution.md`,
`docs/plan/epic-049.md`, the E050 plan/evidence and the server-runtime README.
The linked GitHub requirements review covered the open E050 epic #520, decision
#541 and integration task #543, plus open issue searches in `clickbytes/minimium`
for `executor`, `inline`, `worker pool`, and `RuntimeExecution`.

The product-level E050 statement asks a second host to use existing engine
capabilities through complete public contracts. It does not name a second
server-runtime host requiring alternate execution resources. The other open
search matches describe this decision, the E050 integration ledger, or the epic
goal; none specifies a real alternate executor, fully inline server, or caller
that must inject its own bounded server work service. The documented host
schedule requirement was completed separately by E050-17: it supplies the
ordered operation plan while keeping tick phases and owner-lane publication;
`RuntimeModules::new` prepares that plan, and it does not construct or replace
`RuntimeExecution`. Client content-job ports are also a different client-runtime
service, not evidence that this server pool needs an alternate implementation.

## Options

| Option | Integration and guarantees | Decision |
|---|---|---|
| Keep `RuntimeExecution` concrete | No new boundary or adapter cost. Preserves the synchronous ordered `Executor::map` results consumed by simulation and replication, the ticketed sector-result path, current bounds, stale guards, owner-lane publication, and current shutdown sequence. The known limit remains: a host cannot inject its own executor or make the complete setup inline/thread-free. | **Keep now.** No real alternate consumer is evidenced. |
| Add a future narrow `submit` / `completion` / `cancel` / `drain` port | Requires typed job/result contracts for each proven consumer, adapters for the existing executor and sector services, host composition changes, and owner-side completion validation. Integrating `map` requires preserving synchronous call ordering and output order or deliberately migrating its consumers; sector generation still needs ticket identity, result/install caps, stale rejection and per-world collision revisions. Cancellation must state whether work is interrupted or merely invalidated; drain must report incomplete work by the same absolute shutdown deadline. Bounded admission and completion retention must remain explicit. | **Not authorized by this decision.** A port adds maintenance and adapter costs without a demonstrated alternate implementation. |

The hypothetical port must keep existing pools as the default adapter, publish
mutable results only on the authoritative owner lane, retain bounded admission
and completion queues, preserve cancellation/stale-result guards and tick
ordering, and add no worker/thread cost for the default process. It must not
promise deterministic OS thread scheduling.

## Reconsideration trigger and proof

Reopen AR17 only when a concrete second server host has an actual need to supply
a different bounded execution service or run all affected work inline, and that
need is captured in an accepted requirement. Before implementation, provide a
real consumer and working bounded adapter that handles at least one genuine
server workload with typed completions; a test-only or empty mock is not enough.
The evidence must state resource caps, cancellation/stale identity, publication
owner, worker-failure behavior, and how drain consumes the existing absolute
shutdown deadline. Any implementation needs separate authorization and must
preserve the existing pools as the default adapter.

**AR17 remains an unfixed conditional architecture limitation.** The document
records an explicit deferral; it does not make the host execution resources
replaceable.

## Verification

- Source review: completed at base HEAD `1972371cc67f1a39175d4ec2d0324b45c7a94075`; paths and symbols above were checked against that tree.
- Changed files: this decision, `crates/server-runtime/README.md`, and `docs/plan/epic-050.md`.
- `rtk proxy git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-18 diff --check`: PASS (exit code 0; tracked documentation).
- `rtk proxy git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-18 diff --cached --check`: PASS (exit code 0; all three staged documents, including the new decision).
- Cargo formatting/build/architecture checks: NOT APPLICABLE; no Rust, Cargo, or ownership declarations changed.
- Tests, application/example execution, benchmarks: NOT RUN; root policy defers tests and issue #541 prohibits application/example execution for this card.
