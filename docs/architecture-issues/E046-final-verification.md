# E046: final static verification and runtime backlog

## Step 46: final source tree

The commands below ran in order on 2026-11-07 after the E046-02 and E046-15 follow-up fixes. Testów i scenariuszy uruchomieniowych nie wykonywano.

| Command | Exit | Relevant result |
| --- | ---: | --- |
| `rtk cargo fmt --all` | 0 | Formatting completed. |
| `rtk python scripts/check_file_lengths.py` | 0 | 543 source files, maximum 500 physical lines. |
| `rtk python scripts/check_architecture.py` | 0 | All-feature production dependency graph passed; 2 development edges reported separately. |
| `rtk python scripts/check_static_refactor.py` | 0 | Rust/WGSL layouts, ownership and replaced paths passed. |
| `rtk cargo build --workspace --all-targets --all-features --locked` | 0 | 0 errors, 18 warnings across 5 crates. |
| `rtk cargo build -p minimium-client --no-default-features --locked` | 0 | 0 errors, 28 warnings across 2 crates. |
| `rtk cargo run --manifest-path scripts/wgsl_validator/Cargo.toml --locked` | 0 | All 21 listed shaders valid. |
| `rtk git diff --check` | 0 | No whitespace errors. |

Warnings are unused symbols, private interface visibility and code gated by the desktop feature. They do not indicate a failed build. `Cargo.lock` includes dependency changes required by the current workspace. The tree remains uncommitted; the original untracked `concat.txt` was not changed (`git diff -- concat.txt` is empty). The initial branch, HEAD and complete status are recorded in `E046-initial-git-status.txt`. Step 01–45 evidence is in `E046-step-evidence.md`.

## Contract and removal summary

The 45-row source matrix names each producer, consumer, contract symbol and review evidence. In particular, `AppPaths` owns application paths; the world source contract carries generator revision 5, kind and fingerprint; server admission binds a `Principal` and permissions before mutation; movement history acknowledges only a safe prefix; entity effects flow through atomic world mutation, save format 10 and the replication journal; client replicas publish only complete batches; and `PreparedShadowFrame` plus `GeometryPublicationStamp` connect CPU preparation to one complete GPU geometry publication. The protocol uses version 18. The version checks reject older protocol, save and generator data; no migration path was added.

Replaced paths removed across the owner issues include independent world catalog reconstruction, old player save migration, direct renderer game logic, console atlas generation, stale sector installation and unlocalized property help. The consumers of each replacement are listed in the corresponding issue review and the step matrix. `docs/architecture-execution.md` and owner package READMEs describe the current ownership. The remaining risk is behavior under actual process, network, GPU and input events; compilation and source review cannot establish visual appearance or runtime reliability.

## Step 47: runtime scenario backlog

### Follow-up after Earth launcher report

On 2026-11-07, an actual `start-earth.ps1` invocation exposed two defects beyond the static gate. `games/sample/src/scenario.rs::generate_worlds` now supplies the seed-derived procedural planet definition to its manifest. The Polish Fluent `items-available` select now has one default variant, and `apps/client/src/main.rs` prints the underlying startup error if localization itself fails. The release server/client locked build exited 0 with 17 warnings; the invalid-option client invocation printed its real CLI error; Earth startup proceeded beyond the former panic and stayed running for more than 40 seconds before the manually started instance was stopped. This is a limited startup observation, not acceptance of gameplay or visual behavior. Issues E046-03 and E046-11 were reopened, documented and closed again; all 18 issue states were rechecked before closing the epic.

A later screenshot exposed a false local world input rejection. `LocalServerLaunchConfig::validate_client_world_inputs` now accepts the seed and planet preset already supplied to the server preview and child; `presets/planet/earth.ron` uses generator revision 5. The observed server log recorded `server.ready` and `server.player_connected`. `ConsoleEditor::set_preedit` now ends composition for empty IME preedit, allowing Enter to reach the `exit` command path. Actual keyboard execution of `exit` in the window remains to be checked manually. The launcher build exited 0 with 17 warnings and `git diff --check` exited 0. E046-02, E046-03 and E046-11 were reopened and closed with owner-specific evidence.

These are specified scenarios for a separately authorized runtime test phase, not executed evidence.

| Scenario | Condition | Action | Expected result |
| --- | --- | --- | --- |
| Transfer | Two ready world instances and a portal entity | Interact with the portal during movement and replication | Transfer has one destination, input history resets, and old-world entity records are not published after commit. |
| Save restart | World contains a populated entity, an empty entity, and a player | Stop with final save, restart from save | Entities, next ID, initialization marker and player state match the committed snapshot. |
| Slow network | Delay or drop replication end and movement acknowledgement | Continue movement and entity mutation | Client does not ack or publish an incomplete batch; replay uses only safe input prefix and eventually converges. |
| Cancellation | Queue sector, content, collision and HTTP work | Cancel via interest change, transfer or shutdown | Late results are rejected and worker resources are released. |
| Shutdown | Start active work and pending autosave | Trigger signal, administration and process-owner shutdown separately | Same request path, final save result reaches process status, endpoint and logs close within deadline. |
| PL/EN/IME/DPI | Both clients, Polish and English settings, composition input, DPI change | Switch locale, compose and commit graphemes, resize display | Fluent fallback, separate preedit, complete grapheme edits and consistent hit regions. |
| Shadows | Sector upload, origin rebase and world transfer | Render before, during and after each transition | Publication stamp remains coherent; invalid frames avoid stale shadows. Appearance requires visual review. |
| Portal return | Two portals connect worlds in both directions | Transfer out, then use the return portal | Ready placement and membership follow each transfer exactly once. |
| Replication peers | Fast and slow clients have different full/delta acknowledgements | Mutate while one client awaits full end and change interest | Each client tracks its own pending/acked state; slow client eventually catches up without publishing partial records. |
| Input abuse | Client sends malformed, stale, unauthorized and excessive mutation requests | Exercise admission, movement, combat, chat and entity interaction | Rejections occur before state mutation and preserve session/world context. |
| Cache pressure | Content cache approaches 8 GiB with concurrent HTTP requests | Request new materials, cancel queued work and re-request | Limits and deadlines hold; eviction and cancellation release resources. |
| Save failure | Final save writer fails or exceeds the shutdown deadline | Trigger each shutdown entrypoint | Process reports failed or timed out final save; logs retain the reason. |
