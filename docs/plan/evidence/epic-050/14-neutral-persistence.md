# E050-14 — neutral world-change persistence

## Baseline and ownership

- Base: `d996b097d12fcfbc93840d3d4930c7d5838fb73c`; branch `work/e050-14-neutral-persistence`.
- Owner: `minimium-gameplay` owns the immutable bounded `WorldChangePlan`, typed transition failures, ACK and commitment kinds, and materialized restore state.
- Adapter: `minimium-storage` owns backend validation against persisted state, file/memory queues, codec DTOs, encoding, replay, compaction, and I/O source diagnostics.
- Runtime: `minimium-server-runtime` owns the one preparation/write slot, impact checkpoint reservations, ACK matching, cancellation/shutdown behavior, and proof-gated publication.
- No `.codegraph` index exists. Callers and references were traced with scoped `rg` searches.

## Producer → contract → owner → consumers

| Producer | Contract value | Owner/transition | Consumers |
| --- | --- | --- | --- |
| Tool terrain preparation | Chunks + expected revisions + optional fluid snapshot + catalog digest | Excavation coordinator validates, submits, retains pending plan | Storage adapter; ACK-gated terrain/collision/water publication; action result |
| Inventory request flush | Full revisioned `Inventory` snapshot in one plan | Same coordinator slot and journal | Storage; matching ACK installs inventory; no inventory-owner migration |
| Impact preparation | Body + crater + optional changed chunks/fluid + catalog digest | Same coordinator; impact reservations remain held through exact ACK | Storage; impact projection, terrain, collision, water and replication publication |
| Hydrology checkpoint | Fluid snapshot + catalog digest | Same coordinator and bounded journal queue | Storage; ACK marks the matching fluid revision persisted |
| File or memory restore | `WorldChangeState` | Storage materializes state; runtime checks world/catalog and installs it | Excavation bootstrap, impact restore, terrain restore, hydrology restore |

The file and memory services now expose the same `validate` → `submit` → `poll_acknowledged` contract. `submit` success means bounded queue acceptance only. File ACK is emitted after `sync_all`; memory ACK follows applying the plan to its shared volatile image and says `Volatile`. The runtime checks the opaque identity shared only by clones of the submitted immutable plan and confirms impact lease reservations before constructing `DurablePublication`.

## Field compatibility map

| Previous storage `WorldMutation` field | Gameplay plan field/accessor | Validation and limit retained |
| --- | --- | --- |
| `id: u64` | `id()` | Nonzero and greater than global last mutation ID |
| `world: WorldInstanceId` | `world()` | Chunk, fluid, impact and world-state identities must match |
| `world_seed: u128` | `world_seed()` | Existing world's seed cannot change |
| `expected_chunk_revisions` | `expected_chunk_revisions()` | At most 64 effective results; matching world, no duplicates, current revision must match |
| `resulting_chunks` | `resulting_chunks()` | At most 64; each result has one expected revision and exactly next revision; no duplicates |
| `inventory: Option<(PlayerProfileId, Inventory)>` | `inventory()` | Existing full snapshot, two-slot/equipped/tool-kind checks and next profile revision; owner unchanged |
| `fluids: Option<FluidSnapshot>` | `fluids()` | Same world; revision may not regress; same bounded exact snapshot encoding |
| `impact_bodies` | `impact_bodies()` | At most 4096; valid, same world/seed, monotone transition, total capacity 4096 |
| `craters` | `craters()` | At most `MAX_TERRAIN_EDITS`; valid and immutable by ID; committed body/crater pair checks unchanged |
| `water_catalog_digest` | `water_catalog_digest()` | Existing catalog identity cannot change; geometry/fluid checkpoints require a saved or supplied digest |

Restore maps the same materialized fields: world seed, last world mutation, chunks by coordinates, fluid snapshot, impact bodies by ID, crater stamps by ID, water catalog digest, inventories by profile, global last mutation ID and mutation count. No file offset, record framing, checksum, or replay metadata is exported to gameplay/runtime.

`WorldChangePlan::new` rejects zero IDs, mismatched world values, and existing collection bounds before a plan can be submitted. Storage repeats persisted-state and aggregate-budget validation at queue acceptance and worker commit. The old empty-plan rule remains: a catalog-only mutation is valid; a crater-only plan fails the existing empty-plan guard.

## Encoding and ACK order

The mutation encoder implementation and format constants did not change. `MEXCAV11`, `MEXCHK11`, journal name `excavation.v1`, transaction frame (`u32` little-endian payload length, payload, `u64` checksum), transaction byte cap, checkpoint byte cap, checkpoint threshold, incomplete-tail handling, replay order, and compaction behavior remain in storage. Checkpoint decode now constructs the gameplay-owned materialized world-state type with the same fields and order.

Mutation payload field order remains:

1. Mutation ID, world ID, world seed.
2. Expected chunk revisions, in the existing vector order.
3. Resulting chunk images, in the existing vector order, with samples in the existing order.
4. Optional inventory (profile bytes, revision, equipped slot, two slots).
5. Optional fluid snapshot (revision/config, ordered cells, ordered reservoirs and residuals).
6. Impact bodies, in the existing vector order.
7. Crater stamps, in the existing vector order.
8. Optional water catalog digest.

Checkpoint layout remains global mutation ID/count, ordered worlds and each world's seed/revision/chunks/fluid/bodies/craters/catalog digest, then inventories sorted by profile bytes. `WorldMutation` remains a private storage codec DTO populated from plan accessors. The decoder still materializes current domain values before returning `WorldChangeState`.

Typed refusal branches preserve their former checks: identity/world/catalog mismatches → `IdentityConflict`; optimistic revision mismatch/exhaustion → `RevisionConflict`; quotas and byte limits → `LimitExceeded`; structural/referrer inconsistencies → `InvalidPlan`; queue full/runtime reservation → `Busy`; stopped adapter → `Unavailable`; technical file/codec/backend failure → `Failure` with the original I/O error available as `source`. Runtime maps these kinds directly; it does not inspect `io::ErrorKind` or error text. Memory's existing pre-allocation estimate, encoded-size check and candidate-checkpoint check remain separate from file validation.

The pre-journal preparation pool and impact reservation APIs still use their legacy `io::Result` because they are worker-pool/admission operations, not storage persistence decisions. Their error translation is outside this issue; this migration types only world-change validation, submit/queue acceptance and ACK/failure.

## Guards and preserved behavior

- Queue acceptance remains separate from commit and does not publish state.
- Exact immutable plan identity (including mixed terrain + inventory data) must be returned by the ACK. Same numeric mutation ID with a different plan cannot pass the match.
- Impact checkpoint reservations are still checked before publish; late/stale mismatches fail and request shutdown under the existing impact path.
- Stage cancellation still drops the preparation receiver and releases its exact impact reservations. Accepted writes drain under the current shutdown deadline.
- File ACK follows append + `sync_all` + state apply; best-effort compaction follows the existing durable commit. Memory ACK follows volatile state apply and makes no disk durability claim.
- Inventory remains transaction data. The live owner remains runtime `ExcavationState`; `minimium-world` owns its value type.
- Protocol v21, save v11, generator v6, terrain/content IDs, journal encodings, and all limits are unchanged.

`crates/storage/examples/neutral_world_change.rs` is a real external Cargo example using public `FileStorageBackend`, `MemoryStorageBackend`, and `ExcavationJournalPort` APIs. It constructs a gameplay plan and matches typed validation/queue/ACK errors and the durable/volatile commitment. It is compile-only and was not executed.

`rtk cargo tree -p minimium-storage --edges normal,build --prefix none --format "{p}"` reports the storage graph including `minimium-gameplay`, `minimium-fluids`, `minimium-terrain`, `minimium-world`, and the existing core/platform/simulation/telemetry packages. It contains no `minimium-server-runtime`, `minimium-game-sample`, `minimium-server`, client or renderer package. The locked `cargo check` of the example verifies its actual target graph; its imports are only the public storage/gameplay/core/world APIs. Runtime holds live inventory in its existing `ExcavationState`; `minimium-world` continues to own the inventory value type.

## Changed files

- `Cargo.lock`
- `architecture.toml`
- `crates/gameplay/Cargo.toml`
- `crates/gameplay/README.md`
- `crates/gameplay/src/lib.rs`
- `crates/gameplay/src/world_changes.rs` (new)
- `crates/server-runtime/src/runtime/excavation.rs`
- `crates/server-runtime/src/runtime/excavation/coordinator.rs`
- `crates/server-runtime/src/runtime/excavation/coordinator/advance.rs`
- `crates/server-runtime/src/runtime/excavation/hydrology.rs`
- `crates/server-runtime/src/runtime/excavation/impacts.rs`
- `crates/server-runtime/src/runtime/excavation/inventory.rs`
- `crates/server-runtime/src/runtime/excavation/ports/publication.rs`
- `crates/server-runtime/src/runtime/excavation/preparation.rs`
- `crates/server-runtime/src/startup.rs`
- `crates/storage/Cargo.toml`
- `crates/storage/README.md`
- `crates/storage/examples/neutral_world_change.rs` (new)
- `crates/storage/src/backend.rs`
- `crates/storage/src/excavation.rs`
- `crates/storage/src/excavation/budget.rs`
- `crates/storage/src/excavation/codec/checkpoint.rs`
- `crates/storage/src/excavation/journal.rs`
- `crates/storage/src/excavation/memory.rs`
- `crates/storage/src/excavation/restore_validation.rs`
- `crates/storage/src/excavation/validation.rs`
- `crates/storage/src/memory.rs`
- `docs/plan/evidence/epic-050/14-neutral-persistence.md` (new)

## Static verification

Commands were run from this worktree at the baseline plus this uncommitted diff. No test suite, example execution, app, filesystem service workload, GPU, or benchmark ran.

| Command | Status | Result |
| --- | --- | --- |
| `rtk cargo check -p minimium-gameplay -p minimium-storage -p minimium-server-runtime -p minimium-server --locked` | PASS | Exit 0 |
| `rtk cargo check -p minimium-storage --locked --example neutral_world_change` | PASS | Exit 0; external public consumer compiled |
| `rtk cargo check -p minimium-server-runtime --locked --example embedded_host` | PASS | Exit 0 |
| `rtk cargo fmt --all -- --check` | PASS | Exit 0 |
| `rtk git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-14 diff --check` | PASS | Exit 0 |
| `rtk python scripts/check_architecture.py` | PASS | Exit 0; all-feature graph and four declared feature graphs pass |
| `rtk python scripts/check_file_lengths.py` | PASS | Exit 0; 810 files, max 500 physical lines |
| `rtk python scripts/check_static_refactor.py` | PASS | Exit 0 |
| `rtk cargo tree -p minimium-storage --edges normal,build --prefix none --format "{p}"` | PASS | Exit 0; graph contains the neutral gameplay dependency and no runtime/sample/application/renderer package |
| `rtk proxy cargo check --workspace --locked` | PASS | Exit 0; root integration check, latest source |
| `rtk proxy python scripts/check_feature_contracts.py` | PASS | Exit 0; four independent feature graphs and compile consumers, latest source |
| `rtk proxy python scripts/check_architecture.py` | PASS | Exit 0; feature graphs 37/73/46/49, one dev edge reported separately |
| `rtk proxy python scripts/check_file_lengths.py` | PASS | Exit 0; 810 files, max 500 physical lines |
| `rtk proxy python scripts/check_static_refactor.py` | PASS | Exit 0 |
| `rtk proxy cargo fmt --all -- --check` | PASS | Exit 0 |
| `rtk proxy git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-14 diff --check` | PASS | Exit 0; only line-ending notices |
| Runtime scenarios / byte-comparison execution | NOT RUN | Test/runtime work remains deferred; this compile-only change does not claim byte-for-byte runtime parity |

Static source comparison confirms unchanged codec field order and failure/cancellation paths. Byte-level parity and runtime persistence behavior remain unverified until the separately authorized final testing phase.

## Final source review and deferred scenarios

Coordinator and independent reviewer: PASS on the complete diff. Review corrections restored catalog-only acceptance and distinguished deterministic checkpoint limits from service failures. Runtime no longer imports the private codec DTOs. The following are source traces, not executed scenarios:

| Case | Source path and expected branch |
| --- | --- |
| Revision mismatch | `excavation/validation.rs` returns `RevisionConflict`; coordinator maps it to `Stale`, without publication. |
| Budget refusal | `budget.rs` and `restore_validation.rs` return `LimitExceeded`; runtime maps to `Capacity`, without latching a technical service failure. Existing file/memory budget algorithms remain distinct. |
| Full queue | File `journal.rs::submit` and memory `memory.rs::submit` return `Busy`; coordinator sets its pending slot only after acceptance. |
| I/O failure | File commit returns `Failure` with the original source. The worker stops, and the coordinator records storage failure without constructing a publication proof. |
| Different plan with the same numeric ID | `coordinator/advance.rs` requires clone-shared `same_plan` identity; an independently constructed envelope fails before publication. |
| Late preparation after cancellation | `begin_shutdown` drops the result receiver and releases impact reservations through `WorkSubmission::cancel`; an unsubmitted result cannot publish. |
| Memory lease | `crates/storage/src/memory.rs::open_services` retains the existing exclusive service-group lease; all ports hold it through `Arc<Lease>`. A second opening is rejected until all ports drop. This bootstrap API retains its existing `io::Result`. |
| Shutdown while pending | `excavation/publication.rs::shutdown_excavation` drains accepted writes through acknowledgement under the supplied deadline, then shuts down the journal. Timeout does not establish cancellation of physical I/O. |

No test targets, examples, services or runtime scenarios were executed. Integration commit, CI and main read-back are recorded by the coordinator in issue #537.
