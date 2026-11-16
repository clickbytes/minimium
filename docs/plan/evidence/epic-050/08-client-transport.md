# E050-08 / #531: client message transport

Base: `e2e229850c343aced5f0d3ef00aa9f1c0a7b1f56` (merged #552 and #553).
Branch/worktree: `work/e050-08-client-transport`, `.local/worktrees/e050-08`.
Predecessor #530 was read back CLOSED after its final correction/main verification. The
containing commit/PR identifies the implementation tree checked below.
Transport was rebased after the separate #530 deferred-timeout metadata fix;
that correction is preserved and is not attributed to this transport change.

## Before and after ownership

Before, `ClientSession` owned `TcpStream`, socket options, byte buffers, offsets,
encode/decode calls and the byte boundary of its queued ping. Afterwards:

| Owner / path | Contract and migrated consumers |
| --- | --- |
| `minimium-transport`, `src/client.rs` | `ClientTransportPort`: bounded read, typed front/consume, buffer-only queue admission, actual flush progress, metrics, peer-address metadata, close. No session or gameplay dependencies. |
| `src/client/tcp.rs` | Sole `TcpClientTransport`: nonblocking/TCP_NODELAY setup, existing wire codec, partial frames and writes, buffers and offsets, EOF and concrete I/O errors. |
| `client-runtime/src/session.rs` | Logical state owner takes `Box<dyn ClientTransportPort>`. Keeps authentication/state/context/replica/transfer validation, queue permissions, movement sequence commitment and heartbeat policy. |
| `session/transport.rs` | Narrow mapping of typed adapter results to session outcomes; retains concrete I/O cause, updates cumulative metrics and tracks actual ping completion. |
| `session/receive.rs` | Keeps original poll/content/timeout/readiness/clock ordering; delegates flush/read and consumes typed messages. Content worker ownership unchanged. |
| `session/receive/decode.rs` | Keeps dispatch, selective transition lookahead, deferred world-message handling and stale context rejection. Uses typed peek/consume instead of byte drains. |
| `session/content.rs` | Existing content HTTP preparation obtains only peer address metadata through the port; no other content-job changes. |
| `connection_controller.rs::poll_connector` | Shared default desktop/terminal path constructs configured `TcpClientTransport`, then injects it into `ClientSession::with_presentation`. Connector retains DNS/connect worker and attempt cancellation. |
| `session/failure.rs` | Existing `SessionFailure` moved without semantic changes to keep hand-maintained files under 500 lines. Same public re-export and ticket/cause behavior. |
| `apps/client/src/messages.rs` | Exhaustive mapping of new checked transport-token exhaustion to the existing exhaustion presentation; no protocol-selection option or new user flow. |
| `client-runtime/examples/injected_transport.rs` and `examples/support/local_transport.rs` | External public consumer with a real session and bounded bidirectional local adapter; compiled, never executed. |
| `session/tests.rs::connected_session` | Existing fixture mechanically migrated to injected local transport. No test cases/suites added or run. |

`ClientSession::new` delegates to `with_presentation`; both require a configured
port and return typed handshake queue failure. Presentation and Authenticate are
queued in their original order before first poll. Insufficient alternate-port
capacity returns failure and closes the partially constructed transport, rather
than panicking. The default TCP path remains one implementation for both hosts.

The runtime depends on the existing transport package. Transport's declared
forbidden dependencies now include both runtimes, preventing the reverse edge.
No new crate, executor, transport event bus or duplicate TCP implementation.
Owner READMEs, `architecture.toml` and `docs/architecture.md` reflect actual roles.

## Preserved limits and lifecycle

- Retained inbound and pending outbound byte caps: 64 KiB each. Read chunks remain
  at most 4096 bytes, with 16 KiB newly read per poll. Existing MAX_FRAME_BYTES
  (4100 bytes including the 4-byte header) and codec bytes are unchanged.
- No new write-byte or decoded-frame budget: flush still writes until empty,
  WouldBlock or error; session dispatch still processes complete available frames.
  Interrupted retries. Receive overflow is terminal; queue full is retryable and
  does not accept a frame or commit movement counters.
- Poll order remains content preparation, welcome/heartbeat deadlines, transition
  readiness, flush, bounded read, typed dispatch, time probe/heartbeat queue,
  second flush and clock-frame update. Welcome 5 s, ping interval 10 s and timeout
  20 s remain unchanged. Time-probe timestamp semantics were not moved.
- `ClientFrameToken` is a checked, nonzero, lifetime-local cumulative frame-end
  counter. Accepted/completed totals survive prefix compaction; no wrapping or
  reuse. Exhaustion rejects before accepting bytes. Queue acceptance is not a
  write or server ACK.
- Flush reports `Instant` only when a write crosses the tracked frame end. The
  session starts ping timeout/RTT from that completion, not enqueue. No unbounded
  per-frame completion queue is introduced.
- The typed head remains charged to the TCP receive buffer until explicit
  consumption. Cached decoded values do not advance FIFO. Pending content can
  inspect duplicate/new Prepare, matching Cancel and Error/Bye while leaving an
  unrelated head unconsumed, as before.
- EOF decodes complete buffered frames first, then emits PeerClosed if the
  session is still live. An incomplete trailing frame follows the same baseline
  EOF behavior. Other read errors remain immediate. Write zero preserves
  WriteZero; unsupported environment keeps its separate session outcome.
- `close` discards pending transport data and resets the existing logical state;
  it does not add graceful-drain behavior. Close/drop and constructor failure
  explicitly close the port. Cumulative traffic metrics remain available after
  terminal failure. Adapter lifetime isolation prevents old transport work from
  publishing through a new session.

`DisconnectReason::Io(SessionIoFailure)` now retains `Arc<io::Error>` through
`Error::source`, its original kind and stable `client_runtime.session_io_failed`
code. Equality intentionally remains kind-based, matching the old error-kind
semantics. Protocol/BufferLimit/EnvironmentMismatch outcomes and generic UI
connection-loss formatting remain unchanged.

## Public local consumer

The local adapter has separate bounded peer-pending, session-inbound,
session-outbound and delivered-peer queues, each capped by encoded byte weight.
Read admits at most 16 KiB per poll. All cross-owner queue access uses `try_lock`;
contention produces retry/backpressure or zero progress, never a blocking wait in
session polling. Encoding occurs outside shared locks. Private queue poison is
recovered. Atomic close makes pending input inaccessible immediately; physical
cleanup is best effort without waiting for another owner. Close/drop rejects new
peer input; already delivered peer messages belong to the peer.

The example shows AwaitingWelcome rejecting Ping, buffer acceptance preceding
flush delivery, exact Presentation/Authenticate order, then a host-supplied Bye
after session construction. It consumes the resulting typed UnexpectedMessage
disconnect because the existing session rules reject Bye during AwaitingWelcome.
It does not fabricate a Welcome/world image or claim a connected gameplay run.

## Static source cases reviewed

| Case | Reviewed behavior |
| --- | --- |
| Fragmented frame | Adapter retains partial bytes; typed front remains absent until complete. |
| Multiple frames in one read | Explicit consume removes exactly one frame prefix; FIFO and limits retained. |
| Malformed codec / unsupported environment | Concrete protocol error maps to existing distinct session outcomes. |
| Outbound full | Retryable BufferFull, no accepted token or movement sequence commitment. |
| EOF with partial frame | Complete prefix dispatched; trailing incomplete data discarded with PeerClosed as before. |
| Ping behind partial writes and compaction | Cumulative frame end remains stable; actual crossing starts timeout/RTT. |
| Error after partial progress | Metrics retained, concrete I/O cause attached to terminal result, port closed. |
| Close/drop/reconnect | Old port closes/discards data; new session owns a fresh adapter/token lifetime. |
| Content still pending | Selective typed peek preserves cancellation responsiveness without a second unbounded queue. |
| Alternate adapter handshake cannot fit | Typed construction failure closes port; no panic or half-initialized session escapes. |

Protocol 21, save 11, generator 6, wire bytes and logical admission rules are
unchanged. This completes only AR09 transport. Content jobs remain for #532,
logical clocks for #533, CPU readiness/core for their later owners.

## Verification

Coordinator independently executed the following on the implementation diff at
the base SHA above, in `.local/worktrees/e050-08`. PASS means exit 0.

| Exact command | Status | Evidence |
| --- | --- | --- |
| `rtk proxy cargo check -p minimium-transport -p minimium-client-runtime --locked` | PASS | Both production owners. |
| `rtk proxy cargo check -p minimium-client-runtime --locked --example injected_transport` | PASS | Real public session/port consumer; no execution. |
| `rtk proxy cargo check --workspace --locked` | PASS | Includes shared desktop/terminal production wiring. |
| `rtk proxy cargo check -p minimium-client --no-default-features --locked` | PASS | Terminal graph. |
| `rtk proxy cargo fmt --all -- --check` | PASS | Formatting only. |
| `rtk proxy git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-08 diff --check` | PASS | No whitespace errors. |
| `rtk proxy python scripts/check_architecture.py` | PASS | Production graph plus renderer 37 / terminal 73 / embedded 49 packages. |
| `rtk proxy python scripts/check_feature_contracts.py` | PASS | All three graphs and real CPU/hosted consumers compiled. |
| `rtk proxy python scripts/check_file_lengths.py` | PASS | 783 source files, maximum 500 physical lines. |
| `rtk proxy python scripts/check_static_refactor.py` | PASS | Selected structural declarations only. |
| Tests, application/example execution, network exercise, GPU, benchmarks | NOT RUN | Deferred final testing policy. |

No failed required static check remains. Real fragmentation, disconnect races,
partial-write timing and connected-session interoperability still require the
deferred runtime phase; compilation and source review do not prove them.

## Exact changed files

```text
Cargo.lock
apps/client/README.md
apps/client/src/messages.rs
architecture.toml
crates/client-runtime/Cargo.toml
crates/client-runtime/README.md
crates/client-runtime/examples/injected_transport.rs
crates/client-runtime/examples/support/local_transport.rs
crates/client-runtime/src/connection_controller.rs
crates/client-runtime/src/lib.rs
crates/client-runtime/src/session.rs
crates/client-runtime/src/session/content.rs
crates/client-runtime/src/session/failure.rs
crates/client-runtime/src/session/receive.rs
crates/client-runtime/src/session/receive/decode.rs
crates/client-runtime/src/session/tests.rs
crates/client-runtime/src/session/transport.rs
crates/transport/README.md
crates/transport/src/client.rs
crates/transport/src/client/tcp.rs
crates/transport/src/lib.rs
docs/architecture.md
docs/plan/evidence/epic-050/08-client-transport.md
```
