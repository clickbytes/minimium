# Epic 005 — implementation and verification ledger

This ledger records the implemented integration branch, the changes associated with each execution issue, and the checks actually run. Epic 005 remains open. The integration branch is `epic/005-micro-engine-integration`. The branch is not merged to `main`; aggregate draft PR [#121](https://github.com/clickbytes/minimium/pull/121) still targets `main`.

## Status vocabulary

- **CODE** means the change is present on the integration branch.
- **STATIC** records formatting, compilation, lint, architecture and static contract checks only.
- **FINAL TESTS** means unit/integration/E2E/runtime/GPU/visual/performance verification. These remain **NOT RUN** until the user explicitly starts E005-19.

## Issue, PR, changed files and merge SHA

For PR-backed cards, the linked GitHub **Files** view is the authoritative exact file list for that issue and the full merge SHA is recorded in the table. PRs are merged into the E005 integration branch, not into `main`.

| Issue | Change | Files changed | Integration merge SHA | State |
|---|---|---|---|---|
| [#100](https://github.com/clickbytes/minimium/issues/100) | Integrate drafts #95–#98 and reconcile core exports, modules, entity/universe APIs and server dispatch. | [Aggregate PR #121 file list](https://github.com/clickbytes/minimium/pull/121/files). The card's owned paths are `crates/core/src/lib.rs`, `crates/platform/src/lib.rs`, `crates/platform/src/modules.rs`, `apps/server/src/commands.rs`, `apps/server/src/world_registry.rs`, `apps/server/src/world_registry/constructors.rs`, `apps/server/src/world_registry/diagnostics.rs`, `crates/platform/src/commands.rs`, `apps/server/src/runtime/tick.rs`, `apps/server/src/commands/world.rs`, `apps/server/src/main.rs`, `crates/core/src/entity.rs`, and `crates/core/src/universe.rs`. | [#121](https://github.com/clickbytes/minimium/pull/121), `bcbd561c112cadb9373a8549d128e0f012419d3f` (aggregate head; includes later cards) | CODE; static checks recorded below; final tests NOT RUN |
| [#101](https://github.com/clickbytes/minimium/issues/101) | Modular client command parsing | [PR #122 files](https://github.com/clickbytes/minimium/pull/122/files) | `5ec689ad47286bbd4ce72308c2677b9d049ab15d` | CODE |
| [#102](https://github.com/clickbytes/minimium/issues/102) | Shared traversal policy and typed planet kind | [PR #123 files](https://github.com/clickbytes/minimium/pull/123/files) | `f783d10c5494f471968b82e93e5fac72b2738ec6` | CODE |
| [#103](https://github.com/clickbytes/minimium/issues/103) | Bounded planet presets and validated world construction | [PR #124 files](https://github.com/clickbytes/minimium/pull/124/files) | `cbbfdfc7a43d9cb95db7cf1a03dbcfe330abbffe` | CODE |
| [#104](https://github.com/clickbytes/minimium/issues/104) | Incremental sector pin reconciliation | [PR #125 files](https://github.com/clickbytes/minimium/pull/125/files) | `26fbed67eae92e7bf5c48463011f6e9f0a8d1ac8` | CODE |
| [#105](https://github.com/clickbytes/minimium/issues/105) | Incremental persistence totals | [PR #128 files](https://github.com/clickbytes/minimium/pull/128/files) | `164d3c8845857a9fd3cc65c05f457e3aad2614b7` | CODE |
| [#106](https://github.com/clickbytes/minimium/issues/106) | Runtime module systems by stage | [PR #127 files](https://github.com/clickbytes/minimium/pull/127/files) | `1dd8e22d844f1732596b7fcbad90d8619cc97fd5` | CODE |
| [#107](https://github.com/clickbytes/minimium/issues/107) | Entity lifecycle and sector persistence | [PR #134 files](https://github.com/clickbytes/minimium/pull/134/files) | `69451be0e827dc0a5f16077744356dd28217834d` | CODE |
| [#108](https://github.com/clickbytes/minimium/issues/108) | Explicit replication policy and bounded candidate staging | [PR #138 files](https://github.com/clickbytes/minimium/pull/138/files) | `bcbd561c112cadb9373a8549d128e0f012419d3f` | CODE; staging only, no component wire message |
| [#109](https://github.com/clickbytes/minimium/issues/109) | Astronomy catalog adapter to SkyState | [PR #126 files](https://github.com/clickbytes/minimium/pull/126/files) | `0c9d5a0e5cb9084b8cc3e1022fe6c9e71bc4bb25` | CODE |
| [#110](https://github.com/clickbytes/minimium/issues/110) | Data-driven bounded celestial halos | [PR #130 files](https://github.com/clickbytes/minimium/pull/130/files) | `2b32e2bef10645ca0259ca5a589acf6ecf51e130` | CODE; shader/GPU runtime NOT RUN |
| [#111](https://github.com/clickbytes/minimium/issues/111) | Opt-in demo universe rendering | [PR #133 files](https://github.com/clickbytes/minimium/pull/133/files) | `68fc5718d2684a878f0fa055516a195aec9709ca` | CODE |
| [#112](https://github.com/clickbytes/minimium/issues/112) | Shared lighting presentation profiles | [PR #132 files](https://github.com/clickbytes/minimium/pull/132/files) | `e0bfed92d1435dde847f433161e92f150839f4fc` | CODE |
| [#113](https://github.com/clickbytes/minimium/issues/113) | Gravity policy from world manifest | [PR #129 files](https://github.com/clickbytes/minimium/pull/129/files) | `124011cccb4ab6e09ace070571bf56f9c75acb0d` | CODE |
| [#114](https://github.com/clickbytes/minimium/issues/114) | Linear HDR composition and one tone map | [PR #136 files](https://github.com/clickbytes/minimium/pull/136/files) | `d4d2f347ecd244988ebdc6ea4eaed95586ec9b97` | CODE; shader/GPU/visual behavior NOT RUN |
| [#115](https://github.com/clickbytes/minimium/issues/115) | Shared fixed simulation step | [PR #131 files](https://github.com/clickbytes/minimium/pull/131/files) | `6987d31f7bb4b2c1dc5ad365519aeb51ea3187d0` | CODE |
| [#116](https://github.com/clickbytes/minimium/issues/116) | Separate client game state and render snapshot | [PR #137 files](https://github.com/clickbytes/minimium/pull/137/files) | `9d3bc401c58ebc4eddf58aa2591f17883f70dfac` | CODE; runtime/render behavior NOT RUN |
| [#117](https://github.com/clickbytes/minimium/issues/117) | Deterministic precise-position batches | [PR #135 files](https://github.com/clickbytes/minimium/pull/135/files) | `25f205c92bc46ec0d58df0c4fd90b0d8d3a691b0` | CODE |
| [#118](https://github.com/clickbytes/minimium/issues/118) | Contract audit, docs and static gates | [PR #139 files](https://github.com/clickbytes/minimium/pull/139/files): `docs/plan/epic-005.md`, `docs/architecture.md`, `docs/worlds.md`, `docs/commands.md`, `README.md`, `scripts/check_architecture.py`, `scripts/check_static_refactor.py` | `e47a327c2beff41b1305b219abd22f0ef217e74d` | CODE; static checks PASS below; final tests NOT RUN |
| [#119](https://github.com/clickbytes/minimium/issues/119) | Final runtime and performance verification | Issue-defined test and evidence paths; see [verification ledger](epic-005-verification.md) | — | IN PROGRESS; workspace tests, Naga and six load scenarios executed; GPU/visual, golden E2E and isolated cost comparisons NOT RUN |
| [#120](https://github.com/clickbytes/minimium/issues/120) | Final review and merge to main | Final PR/documentation paths in issue | — | Blocked on #119 |

## Dependencies

`#100` is the base. The first wave is `#101–#104`, `#106`, `#109`; then `#105`, `#110`, `#113`; then `#107`, `#111`, `#112`, `#115`; then `#108`, `#114`, `#117`; then renderer refactor `#116`; audit `#118`; final tests `#119`; and merge gate `#120`. These cards are present in the integration tree. The issue dependency links remain the source of truth for implementation prerequisites.

## Scope audit notes

- The PR file lists above were compared with their execution cards. E005-14/#114 and E005-16/#116 had their allowed file lists expanded before implementation to cover the HDR shader/output plumbing and the client session/window integration respectively; those added paths are visible in their linked PR file lists.
- E005-08/#108 changed only its four writable paths (`entity.rs`, `entity_state.rs`, `replication.rs`, `metrics.rs`). The protocol `kinds.rs` and `limits.rs` files were read-only and remain unchanged; there is no new wire frame.
- `scripts/check_file_lengths.py` already includes `.wgsl` in its hand-maintained source suffix list and still enforces 500 physical lines. No file-length exception or policy change was needed. `AGENTS.md` was read-only and remains unchanged.

## Delivered contracts and limits

- `EngineModule` registration is explicit compiled Rust registration. It is not dynamic plugin loading or a stable external ABI. The server's current built-in module provides `world list`, `world inspect`, and `entity list` under server-admin scope and a startup diagnostic system.
- `EntityStore` is per world instance. `MEC1` is an opt-in envelope within the existing opaque world-object data. Legacy data remains with its old decoder. Replication is explicitly `ServerOnly` unless a component opts in; E005-08 only stages bounded changed candidates and metrics. No component network frame was added.
- `UniverseCatalog` is a bounded in-memory system/body graph using fixed and simplified circular ephemerides. It is not an N-body or precise-ephemeris replacement.
- The client command registry delegates parser functions to connection, environment, gameplay and interface modules. Client and server command scopes remain separate.
- Protocol remains v13 (`crates/protocol/src/limits.rs`); save remains v9. E005 changes do not add a wire kind or a save version.

## Static evidence at integration tree `bcbd561c112cadb9373a8549d128e0f012419d3f`

| Check | Runner / result |
|---|---|
| Formatting | `cargo fmt --all -- --check` — PASS (exit 0) |
| Workspace compilation | `cargo check --workspace` — PASS (exit 0) |
| Clippy | `cargo clippy --workspace` — PASS (exit 0); existing and staging-related `dead_code`, `too_many_arguments` warnings remain, no lint errors |
| Architecture dependency graph and source boundary scan | `python scripts/check_architecture.py` — PASS on this PR's final source |
| File length | `python scripts/check_file_lengths.py` — PASS, 272 source files, max 500 lines |
| Diff whitespace | `git diff --check` — PASS on this PR's final diff |
| Static refactor ABI/export/parser checker | `python scripts/check_static_refactor.py` — PASS on this PR's final source |
| Unit/integration/E2E, server runtime, shader/GPU validation, visual checks, and performance measurements | **PARTIAL**; workspace tests, Naga and six server load scenarios have results in the E005-19 ledger; GPU/visual, golden E2E and isolated cost comparisons remain **NOT RUN** |

The Cargo checks were run on the code tree that became this integration head; GitHub merge metadata assigns aggregate tree SHA `bcbd561c112cadb9373a8549d128e0f012419d3f`. Do not present compilation or lint as runtime acceptance. The `concat.xz.b64` untracked workspace file is not part of Epic 005 and is intentionally not included in any PR.

## Final gate

E005-19 started after the user's explicit instruction to do everything possible to finish the goal, including review and merge. The exact example phrase **“projekt gotowy do finalnych testów”** was not repeated. Results and outstanding gates are recorded in the [verification ledger](epic-005-verification.md). E005-20 must re-check current `main`, the aggregate PR head and all required gates before merging; only that gate can close the epic.
