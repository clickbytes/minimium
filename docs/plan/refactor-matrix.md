# Refactor decisions and review matrix

Packages are sequential; acceptance of one precedes work on the next. Before assigning a subtask, record its exclusive file ownership and contract. The status of a package is `gotowe`, `w pracy`, `do przeglądu`, or `odebrane`; implementation acceptance is distinct from deferred behavioral verification.

| Package | Decisions and review obligations |
| --- | --- |
| R00 | Plan, matrix, cards, warning baseline, 500-line policy; preserve existing work and data. |
| R01 | Atomic profile creation; zero writes; 32-frame fairness; conflicting options; local address and stable endpoint identity for server report samples, without exposing token. |
| R02 | Typed configuration and choices, startup validation, owned paths and named constants. |
| R03 | Split protocol while preserving v8 exports, fields and limits; protocol owns maximum message size. |
| R04 | Split world, physics, collision, snapshots and map parsers; preserve algorithms, lane merge order and map fingerprint. |
| R05 | Split server and storage; single world owner; rotating 16 KiB/client and 256 KiB/iteration send budgets, preserving partial writes and baseline semantics. |
| R06 | Shared client session; DNS and connect off UI thread under one deadline; one active attempt and stale-result rejection; preserve process ownership. |
| R07 | One typed client command parser, catalog and executor; existing server dispatcher; preserve commands and explicit unavailable results. |
| R08 | Split renderer and console responsibilities, external WGSL, context structs; preserve buffers and actual draw accounting. |
| R09 | Shared English message catalog and typed parameters; Polish glyphs and explicit fallback without per-frame allocations. |
| R10 | Separate tick/frame/RTT histogram buckets, document approximations and units, review allocations and executor phases; allocation instrumentation defaults off. |
| R11 | Split generator, typed scenarios and formats, separate report data from serialization; retain report v2 and manifests, omit tokens. |
| R12 | Static line-limit check and documented exceptions; reconcile documentation; review ownership, unsafe, errors, bounds, timeouts, exports and module use. |

For every package: review changes, run formatting check and scoped compilation and Clippy offline with lockfile; workspace Cargo operations are sequential. After R12 also run workspace checks, release build, server allocation-metrics compilation and `git diff --check`. Do not run tests, the application or generator now. Deferred scenarios include profile races, partial/zero writes, 32-frame boundary, command parity, DNS/timeouts, process ownership, snapshot and persistence compatibility, glyphs, metrics and lanes 1/2/auto. Preserve protocol v8, save v3, world model and user data. Compilation is not proof of playability or performance.

## Detailed acceptance contracts

- R01: profile creation holds a short lock and publishes a complete temporary file atomically; corrupt existing profiles remain errors. Zero writes to nonempty client buffers disconnect after accounting for prior progress. The 32-frame budget yields until the next iteration without disconnect. Reject `--demo` with either `--connect` or `--console`. Server status supplies the actual listener address. All report samples require a literal local `127.0.0.1:PORT` matching both listener and endpoint port, and one unchanged endpoint identity including the memory-only token; mismatches or instance changes produce null samples and token-free diagnostics. Address matching is not cryptographic proof. Preserve report v2 and endpoint formats.
- R02: separate typed client, server and generator configuration, with lane and worker enums; validate once at startup. Preserve defaults, paths and ranges; shared filenames live with their format, constants with their domain. No file or environment configuration reads in tick or frame.
- R03: protocol modules cover models, limits, framing, reading, encoding and decoding, with existing public exports. Protocol owns maximum message size. Preserve v8 field order and bounds; do not allocate solely to silence the large enum warning.
- R04: separate rules, geometry, character movement, collision index and queries, raycast, world state, combat, snapshots, map and OBJ parsing. Keep application exports, algorithms, lane merge order and map fingerprint; files meet line limit.
- R05: separate configuration, startup/shutdown, connection state, receiving, replication, player storage, save format and worker. Main loop makes phase order clear; one world owner. Rotate the first serviced client under initial 16 KiB per-client and 256 KiB per-iteration send budgets. Preserve partial writes, queues and baseline updates. Review shutdown and bounds; budgets require later measurement.
- R06: shared desktop/terminal session owns socket, bounded buffers, states, handshake, heartbeat and message handling and emits typed events. DNS and connection run off UI with a shared deadline for all addresses, at most one active operation, attempt IDs for stale results and no accumulating abandoned resolvers. Preserve cleanup and local process ownership.
- R07: existing server command and single dispatcher remain. Terminal and F1 use one client parser, enum command, catalog, executor and typed result. Catalog owns help, aliases and completion. Remove movement format/reparse; preserve existing commands and return explicit unavailable results such as FPS without renderer.
- R08: separate window lifecycle, input, camera, prediction, GPU resources/pipelines, geometry and rendering; move shaders to WGSL. Split console state, history, HUD, text, atlas and theme. Use small contexts instead of long argument lists. Renderer does not parse commands or manage TCP; preserve reusable buffers and actual draw accounting.
- R09: shared English messages have IDs and typed parameters; domain errors are presentation independent. Command names, protocol and report keys remain language independent. Add Polish glyphs and explicit fallback without per-frame allocation.
- R10: shared histograms use separate named tick, frame and RTT buckets; frame buckets distinguish approximately 8, 16, 20 and 33 ms. Define units, source, missing samples and approximate percentiles. Review tick/frame allocations and executor phase propagation; instrumentation remains off by default and both server builds compile.
- R11: split configuration, scenarios, client setup, workers and statistics; use scenario and report format enums and separate report data from serialization. Preserve scenarios, manifests, v2 fields, valid JSON, generator/server settings and token secrecy. Do not run generator.
- R12: static physical-line limit check with documented specific exceptions; document separate rules for generated files and data assets. Reconcile docs; review ownership, unsafe, errors, bounds, timeouts, public exports and connected modules. Remove redundant alternatives and resolve warnings without unjustified hot-path cost. All cards and matrix entries require review evidence.
