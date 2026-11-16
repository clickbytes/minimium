# E050-04 / #526 — Bounded command read pagination

## Baseline and owner

- Issue: [#526](https://github.com/clickbytes/minimium/issues/526); epic:
  [#520](https://github.com/clickbytes/minimium/issues/520).
- Predecessor: #525 was verified CLOSED; PR #548 is merged at the starting
  `main` commit `ec6538f7da36b0d3ce8d41365d3e3b6baa05c1ff`.
- Branch/worktree: `work/e050-04-pagination`,
  `D:/Git/minimium/.local/worktrees/e050-04`.
- No commit or push was created. This evidence accompanies the complete staged
  patch.
- Owner: `minimium-server-runtime` adapts `WorldRegistry` and `EntityStore`
  state into bounded command read pages. `minimium-gameplay` owns the
  transport-neutral typed request, cursor, error, row and page contracts.
  `minimium-world` owns entity/component membership revisions.

## Read paths and continuation mapping

| Command/read API | Scope and ordering | Cursor invalidation |
|---|---|---|
| `world list [CURSOR]` / `worlds` | World instance ID, ascending | World membership revision |
| `players list [CURSOR]` / `players` | Globally unique `PlayerId`, ascending | Actual player-page membership revision; movement does not invalidate |
| `entity list [CURSOR]` / `entity_rows` | World ID, then entity rows, then player rows; entity ID or player ID within row kind | Registry entity membership, player membership and world membership revisions |
| `entity inspect WORLD ENTITY [CURSOR]` / `components(world, entity)` | Component type ID ascending within the full world/entity pair | Component membership revision; component value changes do not invalidate |
| Module `entities(world, request)` | Entity ID ascending within full `WorldInstanceId` scope | Entity membership revision for the world |

All cursors are typed, opaque values with private fields and bind runtime
identity, query, scope, membership revision(s) and the last key. Text tokens
are versioned, bounded to 256 characters, and parsed by the reader before use.
World and entity identities retain their complete u128/u64 values; player
pagination uses globally unique player IDs rather than packing world IDs into a
key. The mixed list's position key carries row kind and ID while the world is
stored separately in the cursor scope. Cursors fail with a typed stale-cursor
error when runtime, query, scope or captured membership revisions differ.

Requests enforce 1..=256 rows and 1..=1 MiB dynamic data. Readers use ordered
keyset traversal and take at most one extra candidate to decide whether a next
page exists, except the player view, which first snapshots the configured
maximum of 512 active players. A row that cannot fit alone returns
`RecordTooLarge`; otherwise byte or row caps return a continuation. List
formatters put continuation tokens before row text so the token remains
available within the existing 8 KiB reply bound. Existing no-token invocations
still request the first page.

Membership revisions advance only when page membership changes, not when
component values or player positions change. Entity-store revision exhaustion
is checked before mutation. Registry entity batches stage and validate the next
membership revision before publication; join, leave and transfer membership
changes likewise check revisions before applying their mutations. Save/wire
structures and existing world, movement, component-update, and tick behavior
are preserved. No next-world discovery feature is included.

## Static acceptance review

- Empty page/source: the readers return an empty page with no continuation when
  there are no candidates ([read_view.rs](../../../../crates/server-runtime/src/commands/world/read_view.rs)
  worlds/entities take-and-peek paths at lines 62-94 and 103-136; players at
  140-195; mixed entity rows at 199-318; components at 322-389).
- One-row page: request limit `1` is accepted and the readers peek for a next
  candidate after emitting at most that one row
  ([read_views.rs](../../../../crates/gameplay/src/read_views.rs) lines 23-34;
  [read_view.rs](../../../../crates/server-runtime/src/commands/world/read_view.rs)
  lines 78-94, 118-136, 246-318, 353-386).
- Maximum page: limit `256` is accepted by the shared bound; reader row loops
  stop at the request limit and produce a cursor only when another candidate
  exists ([read_views.rs](../../../../crates/gameplay/src/read_views.rs) lines
  23-34; [read_view.rs](../../../../crates/server-runtime/src/commands/world/read_view.rs)
  lines 78-94, 118-136, 246-318).
- More than 256 records: with limit 256, the next candidate yields a cursor; continuation excludes the last emitted key. Exactly 256 final records yield no next cursor (the same take-and-peek and row-limit branches above).
- Over-maximum page: limit `257` is rejected as `InvalidLimit` before reading
  ([read_views.rs](../../../../crates/gameplay/src/read_views.rs)).
- Exact byte cap: row checks reject only when `bytes + row_bytes > max_bytes`,
  so a row/page landing exactly on the limit is accepted
  ([read_view.rs](../../../../crates/server-runtime/src/commands/world/read_view.rs)
  lines 167-178, 256-268, 283-300, 353-367).
- Multibyte names: dynamic player-name accounting uses `String::len()` (UTF-8
  bytes), and mixed player row accounting adds the same byte length before
  accepting the row
  ([read_view.rs](../../../../crates/server-runtime/src/commands/world/read_view.rs)
  lines 167-178, 283-300). This is a static source review; no multibyte scenario
  was executed.
- Removed or unknown world: world-scoped entity/component reads return
  `UnknownWorld` when registry lookup misses
  ([read_view.rs](../../../../crates/server-runtime/src/commands/world/read_view.rs)
  lines 103-113, 322-333); command adapters map that typed error to â€œworld
  instance not foundâ€
  ([world.rs](../../../../crates/server-runtime/src/commands/world.rs) lines
  217-234, 278-301). The repository has no world-removal operation in this owner;
  this check covers the missing-world branch.
- Request construction rejects zero and over-limit row/byte caps.
- Cursor parsing rejects malformed, oversized, wrong-runtime, wrong-query,
  wrong-scope and stale-revision tokens; typed module readers can only parse
  returned tokens, not obtain the runtime's issuer from their read-view API.
- Keyset pages advance from the last emitted key; duplicate IDs in different
  worlds remain distinct in mixed entity rows. Player ordering does not truncate
  world IDs, and component scope retains the complete world/entity pair.
- Oversized first rows return the explicit error. Later rows stop at the byte
  cap and expose the last emitted continuation. CLI metadata advertises cursor
  arguments for all four list/inspect commands.
- Revision updates are separate from component value updates and player
  movement. Revision exhaustion is checked before membership mutation.
- `paged_reads` is a real embedded-runtime consumer with continuation loops and
  typed error handling. It was compiled only; it was not executed.
- Tests, scenarios, server/game/GPU runtime, and interactive CLI behavior were
  not run.

## Static checks and failure history

Commands ran from this isolated worktree using the required RTK wrapper. These
were rerun after formatting and adding this evidence file; all passed with exit
code 0. The RTK binary printed its `cargo check (N crates compiled)` summary.

| Exact command | Result |
|---|---|
| `rtk cargo fmt --all -- --check` | PASS (exit 0) |
| `rtk cargo check -p minimium-gameplay -p minimium-world -p minimium-server-runtime -p minimium-server -p minimiumctl --locked --examples` | PASS (exit 0) |
| `rtk cargo check -p minimium-server-runtime --locked --example paged_reads` | PASS (exit 0) |
| `rtk python scripts/check_architecture.py` | PASS (exit 0; production dependency and feature graphs) |
| `rtk python scripts/check_file_lengths.py` | PASS (exit 0; 772 source files, maximum 500 physical lines) |
| `rtk python scripts/check_feature_contracts.py` | PASS (exit 0; renderer-cpu, terminal-client and embedded-server graphs; examples/apps compiled, not executed) |
| `rtk git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-04 diff --check` | PASS (exit 0) |
| `rtk git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-04 diff --cached --check` | PASS (exit 0; complete staged patch, including this evidence) |

Earlier file-length check failed before the read-view split: gameplay
`commands.rs` measured 526 lines and runtime `commands/world.rs` measured 734.
The read contract and reader were split into dedicated files; the final length
check passes. An earlier `git diff --check` without `safe.directory` exited 1
because Git rejected the worktree ownership; the explicit worktree safe-directory
command above passes. A formatting check also initially identified formatting
changes in `lib.rs`, `hosted.rs` and `transfers.rs`; `rtk cargo fmt --all` was
run and the final formatter check passes.

## Changed files

The complete staged diff lists the implementation, ownership documentation,
command metadata and CLI formatter changes alongside this evidence. No runtime
or visual acceptance is claimed; tests remain deferred by project policy. No
GitHub write, commit or push was performed.
