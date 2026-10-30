# Epic 005 final verification ledger

Final verification started on 2026-10-30 after the user's instruction to do everything needed to finish the goal, including review and merge. The issue's example phrase was not repeated verbatim. This ledger records the actual results; **E005-19 is not accepted** while the failures and unrun scenarios below remain.

Platform: Windows `x86_64-pc-windows-msvc`, Rust 1.99.0, Intel Core i5-12600K (16 logical processors). Integration base SHA: `d8cc4eae6aca1d72ca1427f542b177fa88db365d`. `main` baseline SHA: `2418bbea332c23030055ce2dd6a2bb50f1e2a61d`. New E005-19 test commit: pending.

| Scenario | SHA / command | Result and evidence |
|---|---|---|
| Workspace format, compile and lint | Integration base; `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `cargo clippy --workspace --locked` | PASS (exit 0); Clippy reports nonfatal dead-code and argument-count warnings. |
| Existing workspace tests | Integration base; `cargo test --workspace --locked` | **FAIL** (exit 1): server `world_registry::tests::planet_spawn_can_walk_the_generated_approach_to_the_portal` stopped at `[0.0, 9973.9, 3.019531]`; `portal_transfer_waits_for_readiness_and_round_trips_to_saved_location` failed its `near_portal` assertion. Ten other server tests and all previously executed package tests passed. Cargo stopped at the server failure, so this is not a full workspace PASS. |
| Baseline comparison for both failures | `main@2418bbea332c23030055ce2dd6a2bb50f1e2a61d`; separate detached worktree, `cargo test -p minimium-server --bin minimium-server <test-name> --locked` for each test | **FAIL** for both with the same assertions and the same stopped position for the first test. These failures predate E005; this comparison does not waive them. |
| New entity, universe and module contract tests | E005-19 working tree based on integration base; `cargo test -p minimium-core -p minimium-platform --locked` | PASS: 6 core and 3 platform tests; new cases cover ID restoration and migration, duplicate type/name rejection, orbit/occlusion, transactional module installation and command collision. Record final commit SHA after PR. |
| Server/client E2E, save migration and wire golden cases | — | NOT RUN after workspace failure. |
| Naga/WGSL, GPU Surface and Overview, visual reference images | — | NOT RUN. |
| Streaming, precise batch, halo/HDR cost and 2/100/500 player load measurements | — | NOT RUN. Existing load scenario TOML files remain unchanged. |

The two portal failures require a repair with its owning issue and a rerun at the repaired SHA. Until then PR #121 stays draft and E005-20 cannot merge to `main`. No baseline performance claim is inferred from the available unit tests.
