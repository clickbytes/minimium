# E050-15 — private equipment and profile inventory owner

## Preflight

- Worktree: `D:/Git/minimium/.local/worktrees/e050-15`
- Branch: `work/e050-15-inventory-owner`
- Before SHA: `2099b7f5a82138318d6351ed3406f61182b90f80`
- Validation SHA: `2099b7f5a82138318d6351ed3406f61182b90f80`; source and evidence are uncommitted changes on this base.
- Before working tree: clean.
- Predecessor #537: reported CLOSED, PR #560 merged, CI 43960361118 successful, and its merge SHA is this branch base.
- CodeGraph index: absent; callers were traced with scoped `rtk rg` searches.
- Scope read: root and `crates/server-runtime/AGENTS.md`; `architecture.toml`; world, simulation, server-runtime and storage READMEs; safe-refactor, world-consistency and verify skills; E050-14 evidence.
- #538 and #520 were read through the clickbytes GitHub connection.

## Before and after owner map

| Flow | Before | After |
| --- | --- | --- |
| Restore | `Excavation::from_storage` copied `WorldChangeState.inventories` into `ExcavationState.inventories`; the restore ledger retained the same map. | Runtime composition takes `restored.inventories` into private `Equipment.confirmed`; excavation retains the world-change restore ledger only. `PlayerStore` remains separate. |
| Profile lookup | `ServerRuntime::excavation_profile` scanned active connection identities. | `ServerRuntime::player_profile` in `runtime/equipment.rs` reuses that scan; no second profile/session index exists. |
| Grant/equip | `excavation/inventory.rs` read the committed map, overlaid coordinator candidate/request state, and called `ensure_items` / `equip`. | `Equipment::request_change` validates grants/equip and stages the actor/context desired candidate. Admission still checks Gameplay and DeveloperTeleport before grants. |
| Tool admission | `excavation/admission.rs` read inventory and stored `ToolUseGate` in `ExcavationState`. | It reads confirmed equipment and asks Equipment to prepare through the same configured `ToolUseGate` profile, first-hit query, context and monotonic milliseconds. |
| Combat | `combat.rs` called `player_inventory`, which read `ExcavationState.inventories`. | Combat reads the confirmed profile snapshot from Equipment. Missing profile still reads as default inventory. |
| Tool/action result | `ExcavationState.gates` finished admitted actions when runtime or publication recorded results. | `Equipment` owns gates; runtime and publication result paths both finish the same gate only for its matching actor/context. |
| Transaction reservation | Coordinator pending plan supplied an inventory overlay through `pending_inventory(profile)`. | Before an inventory plan is submitted, Equipment reserves the candidate by mutation ID, profile and expected confirmed revision. Coalesced actor requests remain separate. The only route to preflight is coordinator publication after the existing `ack.plan.same_plan(&pending_plan)` check. |
| ACK/publication | `ports/publication.rs` inserted inventory into `ExcavationState` while publishing terrain, fluid and impact state. | Publication preflights Equipment before world mutation, completes existing fallible stages, then moves the candidate into confirmed state before action results and replication. Error, stale or mismatched ACK paths release only the reservation for the coordinator's pending mutation ID. |
| Replication | `excavation/streaming.rs` read the confirmed map directly. | Streaming reads only Equipment's confirmed profile snapshot; pending candidates are never replicated. |
| Disconnect/context/shutdown | Excavation retained gates by connected actor and pruned pending requests by current input context. | Equipment clears actor requests/gates on disconnect and retains the same context pruning. Accepted inventory reservations are not cleared on disconnect and remain until matching ACK/error; confirmed profile state survives world change and reconnect. The existing shutdown drain handles accepted journal work. |

## Persistence and compatibility

Inventory remains a profile-keyed, revisioned snapshot in the existing excavation journal. No PlayerStore snapshot copy, storage codec/journal change, wire message, item ID, slot count, tool catalog, balance or permission rule was changed. Two item slots and existing item/equip validation remain in `Inventory`; application-provided grant slots remain catalog-validated by startup composition. Queue acceptance does not confirm inventory. Only the exact #537 opaque-plan ACK does.

The compact Equipment reservation stores one candidate and its mutation/profile/revision keys; it does not clone the world-change plan's terrain/fluid payload. The coordinator retains the actual submitted plan and performs its unchanged opaque identity check before authorizing publication. Existing excavation publication failure behavior is retained: publication errors request shutdown; this change does not claim to roll back already-installed world stages or cancel an accepted journal write after disconnect.

## Changed paths

- `crates/server-runtime/src/runtime/equipment.rs` — new private owner for the sole confirmed map, desired requests, grant/equip validation, gates, reservation, ACK preflight and commit.
- `crates/server-runtime/src/runtime.rs` — owner composition and transfer of restored inventories out of excavation restore state.
- `crates/server-runtime/src/runtime/excavation.rs` — removed confirmed/request/gate state; profile lookup and action result forwarding.
- `crates/server-runtime/src/runtime/excavation/inventory.rs` — Equipment grant/equip operations and existing journal submission path.
- `crates/server-runtime/src/runtime/excavation/admission.rs` — confirmed-only tool admission and owner-held gates.
- `crates/server-runtime/src/runtime/excavation/coordinator.rs` and `coordinator/advance.rs` — exact pending mutation reservation lifetime and cancellation at ACK rejection.
- `crates/server-runtime/src/runtime/excavation/ports.rs` and `ports/publication.rs` — reservation access, preflight before world stages, confirmation after fallible stages.
- `crates/server-runtime/src/runtime/excavation/publication.rs` — Equipment passed to the existing owner-lane transaction advance.
- `crates/server-runtime/src/runtime/excavation/streaming.rs` — confirmed snapshot replication through Equipment.
- `crates/server-runtime/src/runtime/combat.rs` — confirmed equipment combat gate.
- `crates/server-runtime/src/runtime/network.rs` — actor-scoped equipment cleanup on disconnect.
- `crates/server-runtime/README.md`, `architecture.toml` — ownership and data-flow declarations.

## Canonical changed-file list

- `architecture.toml`
- `crates/server-runtime/README.md`
- `crates/server-runtime/src/runtime.rs`
- `crates/server-runtime/src/runtime/combat.rs`
- `crates/server-runtime/src/runtime/equipment.rs`
- `crates/server-runtime/src/runtime/excavation.rs`
- `crates/server-runtime/src/runtime/excavation/admission.rs`
- `crates/server-runtime/src/runtime/excavation/coordinator.rs`
- `crates/server-runtime/src/runtime/excavation/coordinator/advance.rs`
- `crates/server-runtime/src/runtime/excavation/inventory.rs`
- `crates/server-runtime/src/runtime/excavation/ports.rs`
- `crates/server-runtime/src/runtime/excavation/ports/publication.rs`
- `crates/server-runtime/src/runtime/excavation/publication.rs`
- `crates/server-runtime/src/runtime/excavation/streaming.rs`
- `crates/server-runtime/src/runtime/network.rs`
- `docs/plan/evidence/epic-050/15-inventory-owner.md`

## Static verification

Commands ran from `D:/Git/minimium/.local/worktrees/e050-15` with RTK. Exit codes below are actual process exit codes.

| Command | Exit | Status |
| --- | ---: | --- |
| `rtk cargo check -p minimium-server-runtime -p minimium-server -p minimium-storage -p minimium-game-sample --locked` | 0 | PASS |
| `rtk cargo check -p minimium-server-runtime --locked --example embedded_host` | 0 | PASS |
| `rtk cargo fmt --all` | 0 | PASS; formatter applied before final check |
| `rtk cargo fmt --all -- --check` | 0 | PASS |
| `rtk git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-15 diff --check` | 0 | PASS |
| `rtk rg --count '^'` over changed Rust files | 0 | PASS; highest file is `runtime.rs` at 476 lines; new `equipment.rs` is 264 lines; all changed Rust files are below 500 physical lines |
| Unit, integration and end-to-end suites; running `embedded_host`; runtime gameplay; GPU or benchmark runs | — | NOT RUN, deferred by project policy and #520 |
| `rtk proxy cargo check --workspace --locked` | 0 | PASS; coordinator, finished in 21.54 seconds |
| `rtk proxy python scripts/check_feature_contracts.py` | 0 | PASS; four graphs (37/73/46/49 packages) and four real compile consumers, never executed |
| `rtk proxy python scripts/check_architecture.py` | 0 | PASS; production/target/build edges, one dev edge reported separately |
| `rtk proxy python scripts/check_file_lengths.py` | 0 | PASS; 811 sources, maximum 500 physical lines |
| `rtk proxy python scripts/check_static_refactor.py` | 0 | PASS; selected declarations and paths, not semantic ownership proof |
| `rtk proxy cargo fmt --all -- --check` | 0 | PASS; coordinator final check |

## Residual verification limits

Coordinator and independent source review: PASS on the complete owner/caller migration and compact reservation lifecycle. The final profile helper relocation preserves the existing connection identity lookup. The #537 gameplay/storage contract and codecs are unchanged. Integration commit, CI and main read-back are recorded by the coordinator in issue #538.

Compilation and source review do not prove gameplay/runtime scheduling or persistence I/O behavior. Runtime acceptance remains deferred, including equip during pending tool use, stale revision, journal rejection, wrong-plan ACK, accepted-write disconnect/reconnect, idempotent grant, full budget and error shutdown. The source traces above establish who retains confirmed/pending state and releases the reservation; none of these scenarios was executed.
