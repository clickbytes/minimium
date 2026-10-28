# Epic 002 — verification ledger

On 2026-10-28, Windows x86_64-pc-windows-msvc, rustc 1.99.0 (b940084d7 2026-09-28), integration head `8043f24cf0a2c5cad8495024d7a84e8b3d549274`, main `53314514dac1456744af909aa4a1ea910458d111`. Main was an ancestor of the integration head. These checks apply to this head before this documentation-only ledger change.

| Gate | Command | Result |
| --- | --- | --- |
| Format | `cargo fmt --all -- --check` | PASS |
| Types | `cargo check --workspace --locked --offline` | PASS |
| Static lint | `cargo clippy --workspace --locked --offline` | PASS with existing warnings |
| Release build | `cargo build --workspace --release --locked --offline` | PASS |
| Allocation feature build | `cargo build -p minimium-server --features allocation-metrics --locked --offline` | PASS |
| File policy | `python scripts/check_file_lengths.py` | PASS, 104 source files |
| Dependency graph | `python scripts/check_architecture.py` | PASS, workspace graph only |
| Integration diff | `git diff --check origin/main` | PASS |

Clippy still reports `large_enum_variant` for `ServerMessage` and `SessionEvent`, as in the baseline; no boxing was added to silence it. Incremental compilation reports failed hardlinks and copies cache files instead; Git reports missing commit-graph files. The integration name-status review found deleted source files `message_dispatch.rs` and core `commands.rs`, whose responsibilities moved to runtime and platform respectively; no user data assets are deleted. Static checks and reviews do not demonstrate runtime behavior, compatibility or performance. Individual task evidence is recorded in issues #15–#29 and [the review](epic-002-review.md).

## Final phase — NOT RUN

This phase requires a separate explicit user instruction that the project is ready for final testing. Record commands, environment, SHA, observations and failures when authorized. No application, test or generator was run for this ledger.

- Two clients against one server; UI console, prediction and interpolation.
- Handshake, EOF, partial/zero writes, backpressure, timeout, reconnect, DNS and cancellation.
- Snapshot v2 migration and v3 save/restart, errors, lock races and owned/attached process cleanup.
- Movement, collision geometry, combat and full/delta snapshots.
- Equivalent authoritative inputs and outcomes for lanes 1, 2 and auto with at least 32 players.
- All 18 clustered/dispersed manifests at 2, 100 and 500 clients and lanes 1, 2 and auto; report v2, token isolation, CPU, RSS and optional allocation phases.

PR #31 remains draft and issue #30 and epic #14 remain open until the final phase passes and the epic PR is merged to main. Suitable 500 MHz hardware and a compatible target have not been established; record actual hardware and limitations instead of claiming the research target was met.
