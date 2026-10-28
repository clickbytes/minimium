# Epic 002 — verification ledger

**Current status: NOT RUN.** Nothing below is a completed check until an implementer adds command, SHA, timestamp, environment and outcome.

| Gate | Command | Status | Evidence |
| --- | --- | --- | --- |
| Format | cargo fmt --all -- --check | NOT RUN | — |
| Types/build | cargo check --workspace --locked | NOT RUN | — |
| Static lint | cargo clippy --workspace --locked | NOT RUN | — |
| Release | cargo build --workspace --release --locked | NOT RUN | — |
| Feature | cargo build -p minimium-server --features allocation-metrics --locked | NOT RUN | — |
| File policy | python scripts/check_file_lengths.py | NOT RUN | — |
| Dependency graph | python scripts/check_architecture.py (after E002-14) | NOT AVAILABLE | — |
| Diff | git diff --check | NOT RUN | — |
| Final functional | explicit final test phase after user instruction | DEFERRED | — |
| Final performance | 18 loadtest scenarios after user instruction | DEFERRED | — |

Task PR links, observed warnings and results must be recorded with exact SHA and setup. Historical Epic001 compilation does not prove Epic002 correctness. No runtime tests, app or generator while AGENTS.md defers the test phase.
