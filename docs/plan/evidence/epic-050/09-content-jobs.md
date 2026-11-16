# E050-09 / #532: host-owned content jobs

Base: `35480989ca865c8ea3b9b58a86efa0686b489de6` (merged #554).
Branch/worktree: `work/e050-09-content-jobs`, `.local/worktrees/e050-09`.
Predecessor #531 was read back CLOSED after merge/main/CI verification. The
containing commit and PR identify the implementation tree checked below.

## Ownership and data flow

```text
server message -> ClientSession logical request/ticket/deadline
  -> injected ContentJobsPort.submit -> accepted handle or Busy/rejected
  -> apps/client HostContentJobs slot -> real resolver/water or HTTP/cache worker
  -> typed ticketed completion -> ClientSession validation
  -> pending/active world publication OR material handoff -> existing GPU ACK
```

Before, the session owned both job families' JoinHandles, cancellation flags,
resolver and cache configuration. Now the session owns logical requests,
accepted cancellation handles, deadlines, result validation and publication.
The application adapter owns actual workers, resolver/cache configuration,
cooperative cancellation and draining resources. No arbitrary-closure executor,
new crate, second active-world owner or generic string event bus was introduced.

| Path / symbols | Owner and migrated contract |
| --- | --- |
| `client-runtime/src/content_jobs.rs` | Public World/Material request, ticket, accepted handle, completion, poll, cancellation and shutdown statuses; `ContentJobsPort` and explicit `ContentJobsSession` identity. |
| `PreparedWorldContent::prepare` | Private result fields bind a `ResolvedWorldContent` to its base-only water catalog using existing prepared terrain. Pure CPU preparation; called by host work, not by session poll. No duplicate terrain validator or procgen contract change. |
| `session.rs` constructors/close/drop | Require an injected content service with nonzero session identity; retain logical state and cancel accepted handles on close/drop. No worker, cache path/budget or OS spawn. |
| `session/world_jobs.rs` | Issues checked world request IDs, retries Busy, tracks accepted work, rejects stale/mismatched results, preserves timeout/ticket association and invokes existing final install validation. |
| `session/receive.rs::install_resolved_world_content` | Session remains final authority for ticket, context, transition, manifest/map fingerprint, profile and Earth identity before active/pending publication. |
| `session/receive/decode.rs` and transition dispatch | Keep deferred-message and newest-transition behavior; cancellation is requested through the accepted job handle. Preserve the #553 deferred-timeout metadata fix. |
| `session/content.rs` | Logical material offer/capability/hash/climate/deadline, service request/completion and package identity validation; existing host GPU acknowledgment remains separate. |
| `session/failure.rs` | Retains typed causes and world/material tickets; retry-hint formatting preserves fields instead of flattening a failure into a new string-only error. |
| `connection_controller.rs` | Installs one shared service before connection attempts, rejects replacement, derives fresh session IDs from checked AttemptId, wires all sessions to it, drains against existing shutdown deadline. |
| `apps/client/src/content_jobs.rs` | Default real host adapter: one world and one material slot, real thread ownership, ticket matching, cancelled-slot reaping, nonblocking polling and deadline-limited shutdown. |
| `apps/client/src/content_jobs/workers.rs` | Original world resolver/base catalog and material HTTP/cache installation bodies, with existing AssetIoError/GenerationError causes. |
| `apps/client/src/bootstrap.rs`, `main.rs` | Shared desktop/terminal bootstrap constructs the same service with application paths, cache budget and SampleWorldContentResolver. No frontend-only replacement or GPU dependency. |
| `client-runtime/examples/injected_content_jobs.rs`, `support/local_content_jobs.rs` | External real session requests world preparation, bounded local host takes request, builds public prepared data, completes ticket, then session validates/installs it. |
| `examples/injected_transport.rs`, `session/tests.rs` | Previous public consumer and existing fixture migrated to explicit service injection; no test cases/suites added. |

`WorldContentResolver` and `SampleWorldContentResolver` keep their established
contracts; execution moved to the host. `PreparedTerrainSource` and
`ClientTransportPort` implementations are unchanged. Controller connector/admin/
local-server workers remain legitimate declared adapters, outside this session
content boundary.

## Bounds, identity and failure semantics

- Default service has exactly one world slot and one material slot across all
  reconnects of its controller. Accepted work, completed undrained results and
  cancelled workers still running occupy those slots. Busy accepts nothing; a
  session retries within its logical deadline. Cancelled finished work is reaped
  on other operations even when the old session no longer polls its ticket.
- World tickets retain session/request/context/transition/content revision;
  requests also retain manifest and expected map fingerprint. Material tickets
  retain session/request/manifest hash, with requested endpoint/compression in the
  request. Counters reject overflow. Session IDs must never be reused during the
  lifetime of a shared service; the default checked AttemptId provides that rule.
  ContentJobsSession is consumed, not Clone, and does not claim NonZero alone
  prevents an external host from reusing an ID.
- Accepted handles carry job-scoped cancellation capability. Session records
  atomic intent before any optional port cleanup, so mutex contention cannot
  lose cancellation during close/drop. Requested is not Finished. Host joins
  only after `is_finished`; cancel never waits for running work.
- Live adapter state access uses try-lock, with Busy/Pending on contention;
  destruction uses exclusive state access without waiting for another owner.
  Worker execution and result storage remain bounded. Admission stops before
  shutdown drain; explicit `finish_until` can retry after Pending. Drop after an
  explicit attempt does not introduce another 28-second content wait. All
  components use the same supplied absolute deadline.
- Host worker closures retain resolver/cache configuration and in-flight leases
  until actual exit. Cooperative cancellation cannot interrupt arbitrary blocked
  HTTP/filesystem/cache operations or pure catalog CPU preparation. Pending
  shutdown truthfully reports remaining ownership; dropping a running handle
  detaches rather than pretending the operation ended.
- Complete world results require private `PreparedWorldContent`; its constructor
  derives the water catalog from that exact prepared source with empty overlays.
  A foreign port cannot construct a synthetic missing-planet-catalog result.
  Session still checks final ticket/context/manifest/fingerprint before install.
- Material completion must match the accepted ticket, offered manifest hash and
  compression variant before handoff. Wrong-family results fail explicitly;
  old-ticket results are discarded and cannot set awaiting-GPU or publish worlds.
  Failures of requested world/material work retain their typed source and
  corresponding ticket through disconnect.

## Preserved existing execution

Both families retain the 300-second preparation policy; material deadline still
includes host GPU installation. World active/pending and transition commit/cancel
semantics stay in the session. A queue acceptance is not prepared content, and
prepared material content is not GPU acknowledgment or server admission.

Hash zero still skips HTTP and becomes the existing awaiting-GPU/base-package
path; presentation capability None still bypasses material preparation. Desktop
takes MaterialPackage ownership through GPU import before `finish_content`
queues ContentReady; the material package's cache lease survives that handoff.
Cancellation drops unclaimed results without removing active data. A cache
installation may finish despite cancellation; verified reusable cache artifacts
are not rolled back or deleted by session cancellation.

Material worker keeps bounded manifest fetch and ContentCache.install: existing
manifest/package limits, exact hash/variant validation, configured cache budget
(8 GiB default), digest locks, reservations and leases. No asset-io algorithm or
material bytes/format changes. World generation, Earth geography checks and base
WaterCatalog resolution/math remain unchanged. Protocol 21, save 11, generator 6,
wire bytes, transport fairness, clocks and readiness rules are unchanged.

## Source cases reviewed

| Case | Evidence in the implementation |
| --- | --- |
| Two requests at a family limit | Occupied slot returns Busy; completed-undrained and cancelled-running slots count. The other family remains independently bounded. |
| Cancel, then reconnect | Fresh session ID; old atomic handle remains cancelled, old slot retained until exit and reaped; old completion cannot install into new session. |
| Result for wrong context/session/ticket | Completion ticket checked before use; final ResolvedWorldContent matching and fingerprint checks remain session-owned. |
| Resolver/generation/asset error | Concrete SessionFailure source plus matching world/material ticket retained, including retry hint. |
| Timeout with deferred world message | Existing 300-second decision stays in session; accepted/logical ticket copied before close clears state. |
| Missing/mismatched planet catalog | Private preparation wrapper derives from exact prepared source; no public catalog replacement fields. |
| Wrong material hash/variant/family | Rejected before awaiting-GPU or package handoff. |
| Hash zero / CPU-only capability | Existing no-download/base path and presentation-free path preserved. |
| Cancelled result owns a lease | Result dropped on cancellation reaping; active or already handed-off package retains its own lease until use ends. |
| Mutex contention during cancel/drop | Atomic handle intent is recorded first; cleanup may defer without losing cancellation. |
| Shutdown reaches deadline | Pending returned while work owns resources; explicit retry works, destructor adds no second content wait. |

The external example uses authored dungeon content and public prepared-source
construction, a valid typed Welcome and real ClientSession. It observes Pending
after acceptance, performs preparation only in its separate host phase, submits
completion for the captured ticket, and reads the installed ticket/context after
Connected. It never runs a private app import, fake geography or an arbitrary
closure executor.

## Verification

Coordinator independently ran these commands in `.local/worktrees/e050-09` on
the reviewed implementation at the base above. Every PASS below has exit 0.

| Exact command | Status | Evidence |
| --- | --- | --- |
| `rtk proxy cargo check --workspace --locked` | PASS | Includes desktop and real host adapter. |
| `rtk proxy cargo check -p minimium-client-runtime -p minimium-asset-io --locked` | PASS | Production packages. |
| `rtk proxy cargo check -p minimium-client-runtime --locked --example injected_content_jobs` | PASS | Real external consumer; one unused shared local-transport method warning. |
| `rtk proxy cargo check -p minimium-client-runtime --locked --example injected_transport` | PASS | Previous consumer migrated; four shared content-helper dead-code warnings. |
| `rtk proxy cargo check -p minimium-client --no-default-features --locked` | PASS | Real CPU service composition, no GPU. |
| `rtk proxy cargo fmt --all -- --check` | PASS | Formatting only. |
| `rtk proxy git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-09 diff --check` | PASS | No whitespace errors. |
| `rtk proxy python scripts/check_architecture.py` | PASS | Graphs renderer37 / terminal73 / embedded49; production dependency directions. |
| `rtk proxy python scripts/check_feature_contracts.py` | PASS | All three graphs and CPU/hosted consumers. |
| `rtk proxy python scripts/check_file_lengths.py` | PASS | 789 source files, maximum 500 physical lines. |
| `rtk proxy python scripts/check_static_refactor.py` | PASS | Selected structural declarations only. |
| Tests, examples/applications/network execution, GPU and benchmarks | NOT RUN | Deferred final testing policy. |

Shared example-helper warnings are reported, not suppressed with broad dead-code
allows; their methods are referenced by the other compile consumer. Production
compilation emitted no warnings. No required static failure remains. Real HTTP,
cache contention, reconnect races, cancellation latency and shutdown timing still
require deferred runtime acceptance.

## Exact changed files

```text
apps/client/README.md
apps/client/src/bootstrap.rs
apps/client/src/content_jobs.rs
apps/client/src/content_jobs/workers.rs
apps/client/src/main.rs
architecture.toml
crates/client-runtime/README.md
crates/client-runtime/examples/injected_content_jobs.rs
crates/client-runtime/examples/injected_transport.rs
crates/client-runtime/examples/support/local_content_jobs.rs
crates/client-runtime/src/connection_controller.rs
crates/client-runtime/src/content_jobs.rs
crates/client-runtime/src/lib.rs
crates/client-runtime/src/session.rs
crates/client-runtime/src/session/content.rs
crates/client-runtime/src/session/content_access.rs
crates/client-runtime/src/session/failure.rs
crates/client-runtime/src/session/receive.rs
crates/client-runtime/src/session/receive/decode.rs
crates/client-runtime/src/session/receive/decode/dispatch/transitions.rs
crates/client-runtime/src/session/tests.rs
crates/client-runtime/src/session/world_jobs.rs
docs/architecture.md
docs/plan/evidence/epic-050/09-content-jobs.md
```
