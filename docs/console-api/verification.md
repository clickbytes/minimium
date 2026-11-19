# Acceptance and runtime handoff

Each issue records its baseline, integration SHA, paths, producer, state owner, consumers, static commands and exits, rollback and outstanding runtime observations. Read the integrated change back on main before closing the issue as completed. Reopen the owner issue for a missing requirement. Epic acceptance requires 30/30 completed and all mandatory checks passing on the final main revision. Historical campaigns #470, #384 and #421 remain outside this sequence. [Delivery order](../plan/epic-051.md)

At the migration gate, reconcile the 142 baseline rows, additional operations, all aliases and all findings. For the two clock extensions review the domain registration, bindings, public compile consumer and diff against the predecessor. Check that parser, help, completion and terminal gain both operations through the catalog, and that Rhai subsequently consumes both through typed metadata.

The [inventory](../plan/epic-051-inventory.json) now records 142 frozen rows
plus five bolid, impact and scripting operations, each with an integrated issue, commit and
current Rust registration location. `python scripts/reconcile_epic051_inventory.py`
refreshes those locations and fails when a registration cannot be found.
[The resolution index](discrepancies.md#integrated-resolution-index) maps
C01–C23 and the three additional findings to owner code. Client canonical and
registered alias input uses one catalog binder result; historical `list` is an
explicit compatibility adapter. Client semantic responses no longer use the
`LegacyReported` display wrapper. Runtime comparisons remain NOT RUN.

The [E051 closeout record](../plan/evidence/epic-051/30-closeout.md) lists issue-to-commit ancestry, the #592 repair, consumer coverage, protocol/save versions and the deferred runtime observations. The #597 issue receipt records the required matrix's exact integrated HEAD and exits. Source changes after that receipt require a targeted check of the affected graph.

## Final static matrix

Record the exit and checked SHA for each command plus examples added during E051. [Final acceptance issue](https://github.com/clickbytes/minimium/issues/597)

```sh
cargo fmt --all -- --check
cargo check --workspace --locked
cargo check -p minimium-client --no-default-features --locked
cargo check -p minimium-client --no-default-features --features desktop --locked
cargo check -p minimium-client --features desktop-audio --locked
cargo check -p minimium-client --features console-scripting --locked
cargo check -p minimium-client --no-default-features --features console-scripting --locked
cargo check -p minimium-client --features desktop-audio,console-scripting --locked
cargo check -p minimium-server-runtime --example typed_replies --example typed_host_commands --example paged_reads --locked
cargo check -p minimium-client-runtime --example cpu_client --locked
cargo check -p minimium-renderer --no-default-features --example cpu_frame --locked
python scripts/check_architecture.py
python scripts/check_file_lengths.py
python scripts/check_feature_contracts.py
python scripts/check_static_refactor.py
```

## Runtime handoff

Runtime verification: NOT RUN. The following observations belong to the separately authorized runtime phase.

| Scenario | Observation to capture |
| --- | --- |
| Syntax and aliases | Compare IDs and one execution for each pair. Preserve `say temperatura=20` and diagnose invalid field, method and literal forms. |
| Editor | Complete a token in the middle of a command, navigate signatures, undo and redo one insertion, exercise selection and IME, retain arguments, reject incomplete Enter and 513 bytes. |
| Hosts | Compare PL/EN F1, stdin and terminal values and diagnostics. Exercise capability loss, permission denial and output limits independently. |
| Clock | Compare owner and session sources, requested and achieved rates, owned-server write and joined-player denial. Disconnect after submission and retain unknown outcome. |
| Actor | Pause input, race independent speed/accel patches, jump on ground and while swimming, and change membership before execution. |
| Worlds | Inspect full-width IDs and a stale cursor. Transfer an actor and re-read primary seed. Recheck authorization at each page before revealing instance existence. |
| Water and storage | Compare pending work with a completed tick and received/requested/installed mesh. Observe all-world save enqueue, job completion, backend durability and failure. |
| Rhai | Exercise both scripts, concurrent camera edit, restore conflict, owner failure, cancellation, deadline, output limits and cold/cache timing. |

Use `IMPLEMENTED / STATIC ACCEPTANCE COMPLETE` only after satisfying the final gate. Runtime results require their own evidence.
