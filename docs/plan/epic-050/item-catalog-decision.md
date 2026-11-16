# E050-19 — item catalog and inventory compatibility decision

## Decision

**DEFERRED — keep the current two-tool model.** The repository and its open
requirements do not identify a product scenario that needs a third item, a
variable slot count, or independently authored tool definitions. A stable
definition ID and catalog may be reconsidered when such a consumer is approved.
This decision does not fix AR18: `ToolKind` and the two fixed inventory slots
remain the current contract. Any implementation or format change requires a
separate scoped authorization.

The source review is against main at
`0505a160b8f864b04bbe99e7adda710554abc93e`. Predecessor #541 is CLOSED and its
decision is present at that SHA. The worktree began clean at that SHA on
`work/e050-19-item-decision`.

## Current owner and consumer map

| Concern | Current owner and source | Sample supplied data | Wire/save owner | Client consumer |
|---|---|---|---|---|
| Item identity and inventory value | `minimium-world::items::{ToolKind, ItemRecord, Inventory}` in [`items.rs`](../../../crates/world/src/items.rs). `ToolKind` is the two-value Shovel/Pickaxe enum; `ItemRecord` carries that kind; inventory has exactly two optional slots, optional equipped slot index, and a revision. Slot index is a location, not item identity. | [`DEFAULT_TOOL_GRANTS`](../../../games/sample/src/excavation.rs) supplies one shovel and one pickaxe in those two slots. | The protocol and excavation journal encode the neutral model; storage does not own item meaning. | Client runtime applies confirmed inventory snapshots; the desktop view derives the equipped kind from it. |
| Action parameters and material policy | `minimium-simulation::tools::{ToolDefinition, ToolProfile, ToolUseGate}` in [`tools.rs`](../../../crates/simulation/src/tools.rs). The profile holds two definitions keyed by `ToolKind`; the gate selects from confirmed inventory and checks reach, material set and cooldown, returning the validated definition including radius for terrain application. | [`DEFAULT_TOOL_PROFILE`](../../../games/sample/src/excavation.rs) supplies 3 m reach, 0.6 m radius, 500 ms shared cooldown; Shovel allows soil and Pickaxe soil plus rock. | No tool profile is serialized in the inventory record. The server application passes validated definitions to runtime. | The desktop view reads the sample profile for preview; it does not authorize actions. |
| Confirmed equipment state | Private `Equipment` in [`equipment.rs`](../../../crates/server-runtime/src/runtime/equipment.rs), following E050-15. It owns confirmed inventories by profile, actor requests, one pending reservation and action gates. The runtime constructor takes restored inventories out of excavation restore state into `Equipment`; only matching plan acknowledgement confirms a candidate. | The server composition supplies grant slots and the tool profile. | Excavation journal restore materializes neutral profile-keyed inventories, then runtime installs that map into `Equipment`. | Client replica inventory is a confirmed presentation snapshot, not authority. |
| Model selection and geometry | The sample owns [`tool_model_catalog` and `tool_model_index`](../../../games/sample/src/tool_visuals.rs); the sample list is ordered Shovel/Pickaxe and maps a confirmed kind to its list index. | The sample defines camera-local vertices and colors. | No model geometry or catalog index is persisted or sent in the inventory message. | `apps/client` adapts sample CPU model values to the renderer's bounded input; renderer consumes the selected index and model data, without a second item catalog. |

`architecture.toml` assigns neutral inventory/tool-kind/material-set contracts to
world, bounded tool admission to simulation, and confirmed equipment to server
runtime. `games/sample/README.md` records game-owned profiles, grants and model
data. This keeps item identity separate from parameters, appearance and slot
position.

Concrete client consumers are
[`client-runtime::ExcavationReplica`](../../../crates/client-runtime/src/excavation.rs)
for revision-checked inventory installation,
[`application/desktop_view.rs`](../../../apps/client/src/application/desktop_view.rs)
for confirmed tool selection and preview,
[`terminal_client/events.rs`](../../../apps/client/src/terminal_client/events.rs)
for slot/equipped output, and
[`renderer/excavation_debug.rs`](../../../apps/client/src/renderer/excavation_debug.rs)
for tool notices. The model path runs from sample `tool_model_index` through
[`renderer/camera.rs`](../../../apps/client/src/renderer/camera.rs) to the renderer;
[`renderer/gpu.rs`](../../../apps/client/src/renderer/gpu.rs) adapts the sample
model catalog during presentation setup.

## Current wire and persistence representation

The values below are read from the v21 protocol and v11 storage codecs at the
review SHA. Item codes are codec values, not stable IDs promised for future
catalogs.

| Path | Actual codes and representation | Length and interpretation |
|---|---|---|
| Server → client inventory | [`encoding::excavation::encode`](../../../crates/protocol/src/encoding/excavation.rs) / [`decoding::excavation::decode`](../../../crates/protocol/src/decoding/excavation.rs): outer frame kind `ServerKind::Excavation = 0x99` (see [kinds.rs](../../../crates/protocol/src/kinds.rs)); excavation-message subtype tag `1`; revision little-endian `u64`; equipped byte `0` none, `1` slot 0, `2` slot 1; then exactly two slot bytes, `0` empty, `1` Shovel, `2` Pickaxe. Equipped-empty is rejected. | Inventory state is fixed **11 bytes**: 8 revision + 1 equipped + 2 slot bytes. The subtype tag adds one byte, making the excavation message 12 bytes. Even two empty slots each occupy a byte. `WorldContext` adds 32 bytes; protocol frame header adds 4, so the complete frame is 48 bytes (payload 44). The current protocol version is 21. |
| Client equip request | [`encoding::client::encode_client`](../../../crates/protocol/src/encoding/client.rs) / [`decoding::client::decode_client`](../../../crates/protocol/src/decoding/client.rs): outer frame kind `ClientKind::WorldInput = 8`; payload variant `ClientWorldInput::Excavation = 12`, then `ExcavationInput::Equip = 2`, then presence `0` for unequip or presence `1` followed by one slot-index byte (`0` or `1`). | Context 32 + nested tags 2 + equip payload 1 or 2 = **35 or 36 payload bytes**; the four-byte frame header is additional. No item kind or definition ID is sent in an equip request. |
| Client use request | Same client codec and outer kind `8`: payload variants excavation `12`, use `3`, followed by nonzero action ID little-endian `u64` and three unit-direction little-endian `f32`s. | Context 32 + tags 2 + action ID 8 + direction 12 = **54 payload bytes**; the four-byte frame header is additional. Tool identity is resolved server-side from confirmed inventory. |
| Inventory change journal mutation | [`encode_mutation`](../../../crates/storage/src/excavation/codec.rs) writes an inventory option marker. `0` means absent; `1` is followed by 16 profile-ID bytes and the 11-byte inventory body. The item/equipped codes match the table above. | Option region is **1 byte** when absent, **28 bytes** when present. The overall mutation remains variable because it contains other counted world changes. Journal records wrap the encoded mutation with a 4-byte length and 8-byte checksum; file magic is `MEXCAV11`. |
| Excavation checkpoint | [`encode_checkpoint` / `decode_checkpoint`](../../../crates/storage/src/excavation/codec/checkpoint.rs) writes an inventory count (`u32`) then, per profile, 16 profile-ID bytes and an 11-byte inventory. Duplicate profile entries are rejected on decode. | Count prefix plus **27 bytes per inventory**. The checkpoint payload has `MEXCHK11` magic; [`excavation.rs::compact`](../../../crates/storage/src/excavation.rs) wraps it in the same length/checksum record framing inside the `MEXCAV11` file. |
| Main `world.snapshot` and restore | Storage's [`format/write.rs::encode_snapshot`](../../../crates/storage/src/format/write.rs) and [`format/load.rs::load_bytes`](../../../crates/storage/src/format/load.rs) v11 snapshot codecs store player/world/entity/component records; inventory is not a `world.snapshot` record. It is restored from `excavation.v1` into `WorldChangeState.inventories`, with journal replay applying each latest profile value. Runtime composition takes that map into private `Equipment::confirmed` after storage restore. | The main snapshot has no inventory body. The separate excavation journal/checkpoint is the persistence path for inventory. Save format 11 rejects unsupported versions before worker startup and defines no migration path. |

The fixed 11-byte inventory body and the optional inventory option are distinct
layers. The journal's `None`/`Some` marker changes the option region from 1 to
28 bytes; it does not make inventory slots variable length. Likewise, a slot
byte is not a durable item identifier: it describes the item kind in that fixed
slot.

## Product need and boundary decision

The checked-in sample has two fixed tools, one fixed grant, one two-entry
parameter profile and one two-entry model list. No current requirement asks for
another item kind, a variable slot count, or data-driven item packs. Searches of
the repository and the clickbytes GitHub open issues for `inventory`, `item
catalog` and `ToolDefinition` found no separate product requirement beyond this
decision task and its parent epic. The catalog would currently add another
representation of data the sample already supplies, while widening neutral
identity and touching protocol and journal codecs.

For a future approved need, keep one definition owner and one state owner:

- Neutral world may carry a stable item/definition identity with inventory
  state. It should not own game balance, material policy or model geometry.
- The game/application supplies one validated catalog binding stable IDs to
  action parameters and presentation/model definitions. Server authority uses
  the server-selected catalog; client presentation may consume its matching
  public description but cannot approve an action.
- The private runtime `Equipment` remains the sole confirmed inventory owner.
  Storage persists identity and state through its codec. Renderer and console
  consume supplied views and do not keep alternative catalogs.

### Separate future decisions

**A — stable IDs/catalog for the existing two definitions.** A future task could
replace the current kind identity internally while retaining the two slots and
the current meanings. It may preserve v21/v11 bytes only if an explicit mapping
keeps Shovel and Pickaxe at their existing codes (`1` and `2`), empty at `0`,
and leaves equipped and slot encodings unchanged. It must show byte-for-byte
compatibility at the codec boundary before claiming unchanged encoding. No such
IDs or catalog exist today.

**B — a new kind or a different slot count.** This changes the accepted identity
set, slot semantics, or record cardinality. Existing clients reject unknown item
codes, and a larger slot count changes the fixed two-slot body unless a new
versioned representation is defined. Even if a new kind fits an unused byte
value without changing byte width, that changes protocol/save meaning and
requires an explicit compatibility decision. Do not promise B can preserve
current encoding.

Before implementation, a scoped contract must settle these compatibility rules:

- Unknown or missing definition IDs fail closed at catalog validation or record
  decode. Do not silently remap to a different item or drop inventory state.
- Duplicate IDs in one catalog are rejected deterministically before use.
- Client/server catalog mismatch is detected using the server-selected
  canonical catalog fingerprint; the server remains authoritative and the
  client must not preview a mismatched definition as if confirmed.
- Unsupported save versions are rejected before replacing files or starting
  persistence workers. A migration, if needed, is separately authorized and
  must define rollback and old-world handling.
- Restore is idempotent: replaying the same accepted state cannot mint a new
  revision or duplicate an item. Duplicate profile entries in a checkpoint stay
  invalid; journal order must continue to select the last committed value.
- Fingerprints use a canonical, versioned ordering and encoding of stable IDs
  and gameplay parameters, with documented units and validation. Model-only
  presentation data is either deliberately excluded or separately identified.
  No such catalog fingerprint currently exists.

These are acceptance requirements for a future change, not implemented messages,
IDs, codecs or migration behavior in this decision.

## Reopen trigger and authorization

Reopen AR18 only when a product requirement names an actual second content set,
new kind, or slot behavior that cannot be represented by the current two-tool
model. The owner must provide the scenario and compatibility matrix before
implementation. A stable-ID-only change can be authorized as a separate
documentation-backed codec migration if it proves unchanged bytes. A new kind
or slot layout needs explicit protocol and save compatibility authorization from
the engine/protocol and storage owners plus game/application composition; it
must update client decode/render consumers and the Equipment owner in the same
complete change. The current decision leaves AR18 unfixed.

## Evidence and verification

- Repository HEAD and source review base: `0505a160b8f864b04bbe99e7adda710554abc93e`.
- Predecessor #541: CLOSED; merged decision at this source SHA.
- E050-15 result: [private equipment/profile inventory owner](../evidence/epic-050/15-inventory-owner.md); confirmed ownership was read from post-15 code, not the previous owner map.
- Source material: `architecture.toml`; world, simulation, protocol, storage and sample READMEs; `items.rs`, simulation `tools.rs`, protocol client/excavation codecs, storage excavation codec/checkpoint, runtime `Equipment` restore and sample grant/profile/model catalog.
- Open GitHub requirements: `repo:clickbytes/minimium inventory`, `repo:clickbytes/minimium item catalog`, `repo:clickbytes/minimium ToolDefinition`; no separate product request found.
- Behavior-preservation review: no IDs, bytes, slots, sample behavior, gameplay policy, owner, renderer model or Cargo source changed. AR18 remains an unfixed P3 limitation by this decision.
- `rtk proxy git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-19 diff --check`: PASS (exit 0; tracked documents).
- `rtk proxy git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-19 diff --cached --check`: PASS (exit 0; all four staged documents including this new decision).
- Changed files: this decision, `crates/world/README.md`, `games/sample/README.md`, and `docs/plan/epic-050.md`.
- Manual link/table review: source-checked by the owner, coordinator and independent reviewer; relative paths verified locally.
- Cargo/format/compile checks: **NOT APPLICABLE** (documentation-only).
- Tests, runtime, examples, GPU and benchmarks: **NOT RUN** (deferred; no behavior changed).
