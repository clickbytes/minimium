# E051 integrated static closeout

Status: **IMPLEMENTED / STATIC ACCEPTANCE COMPLETE** after the final #597 receipt confirms the required matrix on integrated main. Runtime verification: **NOT RUN**. Baseline `fb626d32944b373dd995a5c703ee98eef0db9b35`; pre-closeout integrated source HEAD `cbd22b0bd82248b322a6dec93619ad00123ca811`. The #597 issue receipt records the closeout commit SHA, remote-main readback and exit statuses on that exact revision.

## Ordered integration gate

GitHub readback on 2026-11-20 confirmed each predecessor below `CLOSED/completed`. Each listed SHA is an ancestor of the pre-closeout main HEAD; the same main will receive this documentation. #592 was reopened when its first commit failed the domain-registration proof, then corrected at `fb28099cef4b94a6c15a4753362c70ef4815c7d9`; the inventory provenance was corrected at `cbd22b0bd82248b322a6dec93619ad00123ca811` before its final reclosure.

| Issue | Integration commit | Readback |
| --- | --- | --- |
| [#568](https://github.com/clickbytes/minimium/issues/568) | [`47d5c4b`](https://github.com/clickbytes/minimium/commit/47d5c4befcc8515c7bc975acbc875e2e683f6bfb) | CLOSED/completed; ancestor of main |
| [#569](https://github.com/clickbytes/minimium/issues/569) | [`339e051`](https://github.com/clickbytes/minimium/commit/339e051f5027338f522819a98e71632ba2d5fc24) | CLOSED/completed; ancestor of main |
| [#570](https://github.com/clickbytes/minimium/issues/570) | [`2993820`](https://github.com/clickbytes/minimium/commit/2993820f5236f39e48ddd334ca55a6324e4c866f) | CLOSED/completed; ancestor of main |
| [#571](https://github.com/clickbytes/minimium/issues/571) | [`efb9642`](https://github.com/clickbytes/minimium/commit/efb9642011a898ed16104053a025a4299aa38beb) | CLOSED/completed; ancestor of main |
| [#572](https://github.com/clickbytes/minimium/issues/572) | [`43a672d`](https://github.com/clickbytes/minimium/commit/43a672d5f0f3b9741f27f4aaa9fd7df2dbe3e8e7) | CLOSED/completed; ancestor of main |
| [#573](https://github.com/clickbytes/minimium/issues/573) | [`2bf9799`](https://github.com/clickbytes/minimium/commit/2bf9799ba326d0274dd6069544ac502e5910528f) | CLOSED/completed; ancestor of main |
| [#574](https://github.com/clickbytes/minimium/issues/574) | [`32cc544`](https://github.com/clickbytes/minimium/commit/32cc54418b8b19b23ba15cf5f39993c26488da40) | CLOSED/completed; ancestor of main |
| [#575](https://github.com/clickbytes/minimium/issues/575) | [`89d29fa`](https://github.com/clickbytes/minimium/commit/89d29fa0b71544fb84e25b7a84932f50261805c9) | CLOSED/completed; ancestor of main |
| [#576](https://github.com/clickbytes/minimium/issues/576) | [`1f8c743`](https://github.com/clickbytes/minimium/commit/1f8c743840822c9f11678321c951726b859af21b) | CLOSED/completed; ancestor of main |
| [#577](https://github.com/clickbytes/minimium/issues/577) | [`9c45510`](https://github.com/clickbytes/minimium/commit/9c45510f8c0a7eafa7809d0f43b2e5beb7c7cabd) | CLOSED/completed; ancestor of main |
| [#578](https://github.com/clickbytes/minimium/issues/578) | [`d615775`](https://github.com/clickbytes/minimium/commit/d615775e37ca8a52564b2343ae4a096639712c28) | CLOSED/completed; ancestor of main |
| [#579](https://github.com/clickbytes/minimium/issues/579) | [`5d0f721`](https://github.com/clickbytes/minimium/commit/5d0f72144226d5ad971c8b5725392b5efe37a971) | CLOSED/completed; ancestor of main |
| [#580](https://github.com/clickbytes/minimium/issues/580) | [`ea14a79`](https://github.com/clickbytes/minimium/commit/ea14a795de1778664ad08f4d3c7906b1489bd91e) | CLOSED/completed; ancestor of main |
| [#581](https://github.com/clickbytes/minimium/issues/581) | [`5986479`](https://github.com/clickbytes/minimium/commit/59864795a4aab7b99c6b258c89035ce5c80940ac) | CLOSED/completed; ancestor of main |
| [#582](https://github.com/clickbytes/minimium/issues/582) | [`8d56ba6`](https://github.com/clickbytes/minimium/commit/8d56ba6579a82486b76a4491fcaf45f4f7134f93) | CLOSED/completed; ancestor of main |
| [#583](https://github.com/clickbytes/minimium/issues/583) | [`be8a9d6`](https://github.com/clickbytes/minimium/commit/be8a9d6de256f4b87e28433fe79edd51e77c6296) | CLOSED/completed; ancestor of main |
| [#584](https://github.com/clickbytes/minimium/issues/584) | [`2aa6a4e`](https://github.com/clickbytes/minimium/commit/2aa6a4e87fc938adc0f778b05449591294f1eef9) | CLOSED/completed; ancestor of main |
| [#585](https://github.com/clickbytes/minimium/issues/585) | [`c84e1a6`](https://github.com/clickbytes/minimium/commit/c84e1a6934c4bf58fdc69aa00e70dace0d9b1b44) | CLOSED/completed; ancestor of main |
| [#586](https://github.com/clickbytes/minimium/issues/586) | [`f4ba411`](https://github.com/clickbytes/minimium/commit/f4ba41159827be1cb016c95c45bbf90b62778612) | CLOSED/completed; ancestor of main |
| [#587](https://github.com/clickbytes/minimium/issues/587) | [`9ffc52b`](https://github.com/clickbytes/minimium/commit/9ffc52b920d2a80f024b3d10c1cf82212e0ebda8) | CLOSED/completed; ancestor of main |
| [#588](https://github.com/clickbytes/minimium/issues/588) | [`34dbb8b`](https://github.com/clickbytes/minimium/commit/34dbb8b1238b1a35064904e0a493acc4d797d86e) | CLOSED/completed; ancestor of main |
| [#589](https://github.com/clickbytes/minimium/issues/589) | [`43b86a5`](https://github.com/clickbytes/minimium/commit/43b86a58352f02aa323306b122e39f50c800ae1c) | CLOSED/completed; ancestor of main |
| [#590](https://github.com/clickbytes/minimium/issues/590) | [`073aa34`](https://github.com/clickbytes/minimium/commit/073aa34f04a42857121094690982b7563e2946e0) | CLOSED/completed; ancestor of main |
| [#591](https://github.com/clickbytes/minimium/issues/591) | [`7103de7`](https://github.com/clickbytes/minimium/commit/7103de7e20bbdb0322365d09dd5b306f061012a6) | CLOSED/completed; ancestor of main |
| [#592](https://github.com/clickbytes/minimium/issues/592) | [`e5e7c7a`](https://github.com/clickbytes/minimium/commit/e5e7c7a5426993cb65b5af8c865405555182a466), [`fb28099`](https://github.com/clickbytes/minimium/commit/fb28099cef4b94a6c15a4753362c70ef4815c7d9), [`cbd22b0`](https://github.com/clickbytes/minimium/commit/cbd22b0bd82248b322a6dec93619ad00123ca811) | CLOSED/completed; ancestor of main |
| [#593](https://github.com/clickbytes/minimium/issues/593) | [`6245fc9`](https://github.com/clickbytes/minimium/commit/6245fc93ef171d6bce6194c61e2294cb0c51a899) | CLOSED/completed; ancestor of main |
| [#594](https://github.com/clickbytes/minimium/issues/594) | [`b8db008`](https://github.com/clickbytes/minimium/commit/b8db008de0b045e9ea6f98192ee5e32e97467c96) | CLOSED/completed; ancestor of main |
| [#595](https://github.com/clickbytes/minimium/issues/595) | [`bc34c84`](https://github.com/clickbytes/minimium/commit/bc34c843c14c323aa0ccda1cdeca266bef2c6613) | CLOSED/completed; ancestor of main |
| [#596](https://github.com/clickbytes/minimium/issues/596) | [`78364c1`](https://github.com/clickbytes/minimium/commit/78364c13f1e6694034b707540cb24922541a4c60) | CLOSED/completed; ancestor of main |
| [#597](https://github.com/clickbytes/minimium/issues/597) | Closeout SHA in final issue receipt | Verify integrated main and CLOSED/completed after static gate |

## Contract and inventory

[ADR 0017](../../../adr/0017-public-console-contract.md) is accepted. The audited base used Protocol 22 / Save 12; source at closeout declares Protocol 26 / Save 12. The protocol revisions came with typed player and owner receipts. The inventory contains 142 frozen declarations and five added bolid, impact and scripting operations (147 total). Every row records its registration, owner issue and relevant integration commit; N006 and N007 point to the environment registrar after the #592 correction. The [compatibility matrix](../../../console-api/compatibility.md) covers aliases and legacy spellings. The [discrepancy index](../../../console-api/discrepancies.md#integrated-resolution-index) maps C01–C23 plus module reservation, movement patch and impact reservation to integrated code.

A client operation is parsed into a catalog identity, lowered by owner and kind, and executed through an owner adapter. Admission, queueing, authoritative commit and ambiguous disconnect outcomes stay distinct. Exact u128/u64 identifiers and revisions are retained through typed results. The global clock's server owner and session sample expose distinct provenance. World primary, active and target identities, bounded cursors, pending and committed water, and save job completion remain separate in their respective owners. These are code and contract observations; the runtime scenarios below remain unexecuted.

## Consumer coverage

| Consumer | Static integration evidence | Runtime |
| --- | --- | --- |
| Desktop F1 and developer stdin | Shared client catalog, binder and correlated frontend results; desktop and audio feature graphs compile | NOT RUN |
| Terminal without GPU | Same catalog and typed action path; no-default client graph compiles | NOT RUN |
| Admin CLI and versioned transport | Correlated typed request/result, owner authority checks, compile consumers | NOT RUN |
| Public Rust consumers | Environment time catalog, server typed replies/host commands/paged reads, CPU client and renderer examples compile | NOT RUN |
| Rhai off/on | Default-off feature graph and scripting graphs compile; bridge resolves catalog schema without names of the two clock extensions | NOT RUN |

## Batch 25 proof

The initial #592 implementation commit `e5e7c7a` had per-name frontend edits and did not by itself satisfy the proof. The owner remediation `fb28099cef4b94a6c15a4753362c70ef4815c7d9` placed registration of `world.time.achieved_scale` and `world.time.diagnostics()` in `crates/environment/src/console.rs`, called that registrar from the production client catalog, and replaced the per-name client binder with metadata-based EnvironmentClock read/query lowering. No name-specific addition remains in the F1 parser, help, completion, terminal, property enum or Rhai bridge. The time adapter maps the two domain values to the established session sample. The independent `time_catalog_consumer` compiles against the public environment package.

## Static gate and deferred scenarios

Run the exact command matrix in [verification](../../../console-api/verification.md#final-static-matrix) against the integrated #597 HEAD and record each exit in the #597 issue receipt. Include the environment time consumer and no-default server/runtime examples. `check_agent_contracts.py` is NOT APPLICABLE because no agent instructions or skills changed. `cargo test`, applications, GPU rendering, Rhai evaluation and benchmarks are NOT RUN under the project testing policy.

Later runtime acceptance must cover alias execution, editor/IME/Unicode, PL/EN output, permission and disconnect outcomes, clock sample versus owner source, movement and jump authority, world transfer and full-width IDs, water and save durability, and Rhai workflows/cancellation/budgets. The [handoff table](../../../console-api/verification.md#runtime-handoff) gives expected observations. Documentation rollback is to reopen #597 and the relevant owner issue while preserving these receipts; behavior changes require their respective owner commits to be reviewed individually.
