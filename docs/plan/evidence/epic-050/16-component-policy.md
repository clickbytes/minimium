# E050-16: Component mutation policy evidence

## Worktree and change inventory

- Base: `496ee4706f7ddc3afb66cb39d1be8f83cf075d1f`.
- Branch: `work/e050-16-component-policy`.
- Working directory: `D:\Git\minimium\.local\worktrees\e050-16`.
- Canonical changed paths:
  - `architecture.toml`
  - `crates/gameplay/README.md`
  - `crates/gameplay/examples/world_discovery.rs`
  - `crates/server-runtime/README.md`
  - `crates/server-runtime/examples/owned_components.rs`
  - `crates/server-runtime/examples/support/owned_components_host.rs`
  - `crates/server-runtime/examples/support/owned_components_modules.rs`
  - `crates/server-runtime/src/commands.rs`
  - `crates/server-runtime/src/lib.rs`
  - `crates/server-runtime/src/runtime.rs`
  - `crates/server-runtime/src/runtime/component_ownership.rs`
  - `crates/server-runtime/src/runtime/entity_effects.rs`
  - `crates/server-runtime/src/runtime/excavation/ports.rs`
  - `crates/server-runtime/src/runtime/excavation/ports/publication.rs`
  - `crates/server-runtime/src/runtime/excavation/publication.rs`
  - `crates/server-runtime/src/runtime/impacts.rs`
  - `crates/server-runtime/src/runtime/impacts/owner.rs`
  - `crates/server-runtime/src/runtime/impacts/projection.rs`
  - `crates/server-runtime/src/runtime/loop.rs`
  - `crates/server-runtime/src/world_registry/entity_state.rs`
  - `crates/server-runtime/src/world_registry/tests/spawned_component.rs`
  - `crates/world/README.md`
  - `crates/world/src/entity.rs`
  - `crates/world/src/entity/registry.rs`
  - `crates/world/src/entity/store.rs`
  - `crates/world/src/impact.rs`
  - `docs/architecture.md`
  - `docs/plan/evidence/epic-050/16-component-policy.md`
  - `games/sample/src/portal.rs`

## Scope and ownership

`ComponentDescriptor` now declares `ComponentMutationPolicy::{General, OwnerOnly}`. `ComponentRegistry` validates and stores the owner binding alongside the descriptor; ordinary module component registration preserves the module identity, and built-in BODY registration remains crate-private. `ComponentOwnerId` is opaque and registry-bound. `ServerRuntime` issues host ports for registered module-owned components and owns their world-scoped leases and entity reservations. Ordinary gameplay contexts receive no port or runtime authority.

The generic entity staging boundary is `WorldRegistry::stage_entity_effect_batch` and its shared validator in `world_registry/entity_state.rs`. It reads the registry policy and generic reservation ledger. Owner-authorized effects use the same staging, snapshot-budget preparation, and atomic commit path. Restore uses the same registered descriptor policy through `EntityStore::restore_component`/registry upgrade. The ledger supports multiple owners reserving one entity, requires existing entities on public reservation, and is bounded by the existing persistent-entity capacity. Releasing a port clears its reservations and revokes its clones. Runtime shutdown retains internal impact authority through journal/projection and storage drain, then revokes it. Current `WorldRegistry` has no world-removal API and rejects duplicate instance IDs; any future removal API must revoke that world's lease in the same change.

No component IDs, payload encodings, wire schemas, save formats, replication rules, effect receipt types, or `WorldChangePlan`/ACK/storage contracts changed. No general bypass flag, public mutable runtime store, or native plugin sandbox was added.

## Guard behavior: before and after

| Operation | Before | After |
|---|---|---|
| Ordinary spawn carrying BODY | Rejected before staged spawn | Rejected by registered OwnerOnly policy |
| Ordinary set BODY | Rejected | Rejected by registered OwnerOnly policy |
| Ordinary remove BODY, even absent | Rejected | Rejected by registered OwnerOnly policy |
| Ordinary despawn of pending/live protected entity | Rejected using impact live+pending set | Rejected using generic world/entity reservations before projection and while live |
| Ordinary despawn of entity carrying BODY | Rejected | Rejected by descriptor policy inspection |
| Ordinary set/remove of another component on protected entity | Allowed subject to schema/capacity/entity validation | Still allowed subject to the same validation |
| Ordinary despawn of unprotected entity | Allowed subject to entity validation | Still allowed subject to entity validation |
| Valid owner mutation | No generic owner path | Allowed only through matching registry owner, runtime issuer, world lease, and port; staged/committed atomically |
| Foreign owner/registry/world or released port | No owner-port path | Rejected closed with typed owner errors |
| Persistent restore | Validated descriptor schema/payload | Also requires valid OwnerOnly registration binding; no saved-data format change |

Effect outcomes remain delivered through the existing receipt path. Failed entity effect subgroups remain unpublished; owner batch snapshot-budget failure occurs before commit. Cancellation releases only the new reservation when the entity is neither live nor pending, preserving existing protection.

## Producer, owner, consumers, and lifecycle paths

- Producers: `ModuleRegistrar::component` records the declaring module; `RuntimeModules`/`commands::component_registry` installs that identity with the descriptor. BODY is registered by the private built-in impact registration path. Gameplay `WorldEffect` remains an unprivileged producer.
- State owner: `ServerRuntime` owns `ComponentOwnerAuthority` and its mutable generic reservation ledger. Ports carry opaque identity and weak lease handles; they do not contain independently mutable reservation state.
- Consumers: `world_registry/entity_state.rs` validates spawn/set/remove/despawn; `runtime/entity_effects.rs` commits ordinary and host-authorized batches; registry upgrade/restore validates descriptors; impact projection validates its private owner port against the expected BODY owner.
- Pending/live protection: impact queue setup reserves the body entity before projection and checkpoint submission; failed submit/cancellation releases only if not pending/live. ACK publication validates the checkpoint, publishes projection, then acknowledges. Excavation submission/cancellation/publication follows the same reserve/validate/publish/ack lifecycle. Restore verifies journal/body consistency before projection. Shutdown submits final checkpoints and drains durable work before authority revocation.
- Neutral second owner: `crates/server-runtime/examples/owned_components.rs` composes two independently registered owner components plus a General component through the real server runtime. It covers pending reservation before projection, owner writes, ordinary Despawn/Set/Remove receipt rejection, General Set receipt success, cross-owner and cross-world rejection, multi-owner reservation release, and stale clone rejection. This is compile-only evidence; it was not executed.

## Verification

Commands run from the issue worktree. The initial full-format failure and its correction are recorded separately below:

- `rtk cargo check -p minimium-world --locked` — PASS (exit 0).
- `rtk cargo check -p minimium-server-runtime --locked` — PASS (exit 0).
- `rtk cargo check -p minimium-world -p minimium-gameplay -p minimium-storage -p minimium-server-runtime -p minimium-game-sample --locked` — PASS (exit 0).
- `rtk cargo check -p minimium-server-runtime --locked --example owned_components` — PASS (exit 0; compile only).
- `rtk rustfmt --edition 2024 --config skip_children=true --check crates/server-runtime/examples/owned_components.rs crates/server-runtime/examples/support/owned_components_host.rs crates/server-runtime/examples/support/owned_components_modules.rs` — PASS (exit 0).
- All changed Rust source and example files passed the scoped `rustfmt --edition 2024 --config skip_children=true --check` review after formatting; each changed hand-maintained Rust file remains at or below 500 physical lines.
- Initial integration `rtk proxy cargo fmt --all -- --check` — FAIL (exit 1) because workspace-edition import ordering differed in several modified Rust files. Ran `rtk cargo fmt --all`; it changed formatting only in the already scoped Rust files. Recheck `rtk proxy cargo fmt --all -- --check` — PASS (exit 0). Post-format line audit found no changed Rust file above 500 lines.
- `rtk git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-16 diff --check` — PASS (exit 0).
- Root integration matrix, all from `D:\Git\minimium\.local\worktrees\e050-16`, base `496ee4706f7ddc3afb66cb39d1be8f83cf075d1f`:
  - `rtk proxy cargo check --workspace --locked` — PASS (exit 0, 21.08s).
  - `rtk proxy python scripts/check_feature_contracts.py` — PASS (exit 0; configurations 37/73/46/49 plus four consumers).
  - `rtk proxy python scripts/check_architecture.py` — PASS (exit 0).
  - `rtk proxy python scripts/check_file_lengths.py` — PASS (exit 0; 815 sources, maximum 500 lines).
  - `rtk proxy python scripts/check_static_refactor.py` — PASS (exit 0).
  - `rtk proxy cargo fmt --all -- --check` — initial FAIL (exit 1, workspace-edition import ordering), corrected with `rtk cargo fmt --all`; final PASS (exit 0).
- Cargo dependency graph audits (not CodeGraph; no `.codegraph` index or CodeGraph commands used):
  - `rtk proxy cargo tree -p minimium-server-runtime --edges normal,build --prefix none --format '{p}'` — PASS (exit 0); no sample or renderer dependency.
  - `rtk proxy cargo tree -p minimium-server-runtime --edges normal,build,dev --prefix none --format '{p}'` — PASS (exit 0); includes the pre-existing sample development dependency. The example itself composes only neutral public APIs; no unrelated Cargo dependency change was made.
- Unit, integration, end-to-end tests and example execution — NOT RUN (deferred project verification phase).
- `rtk proxy cargo check -p minimium-server --locked` — PASS (exit 0), run by the root integration owner.
- `rtk proxy cargo check -p minimium-client --no-default-features --locked` — PASS (exit 0), run by the root integration owner.

Runtime/visual acceptance was not performed. Native Rust callers remain trusted host code; the owner port is typed lifecycle/ownership control, not a sandbox for malicious native code.

## Final source review

Coordinator and independent reviewer: PASS on the frozen diff and external consumer. Review corrections covered transactional owner registration, bounded leases/reservations, expected-owner checks in generic staging and impact projection, pending/live cleanup on every queue exit, and preservation of other owners' claims on release. Generic staging and effect execution contain no BODY_COMPONENT or ProtectedImpactEntities references. The initial-content path remains an unprivileged effect producer; restore remains a schema/payload-validating host path. All descriptor literals and existing fixture signatures are migrated.

The independent reviewer ran a read-only diff check (exit 0), but no compilation, format or runtime checks; coordinator checks are listed above. Tests and test targets, services, examples, applications, GPU/visual checks and benchmarks were not executed. Resulting implementation/merge SHA, CI and main read-back are recorded by the coordinator in issue #539.
