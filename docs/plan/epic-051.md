# E051 public console operations

[ADR 0017](../adr/0017-public-console-contract.md) · [Public contract](../console-api/contract.md) · [Compatibility](../console-api/compatibility.md) · [Discrepancies](../console-api/discrepancies.md) · [Verification](../console-api/verification.md)

The baseline for [#567](https://github.com/clickbytes/minimium/issues/567) is `fb626d32944b373dd995a5c703ee98eef0db9b35` (Protocol 22 / Save 12). The integrated source declares Protocol 26 / Save 12 in [limits.rs](../../crates/protocol/src/limits.rs) and [format.rs](../../crates/storage/src/format.rs). All 29 predecessor issues were read back as CLOSED/completed on main; #592 was reopened for its domain-registration proof, corrected in `fb28099`, then reclosed. [Closeout evidence](evidence/epic-051/30-closeout.md) records each commit and the final static gate. Runtime verification remains NOT RUN.

| Batch | Issue | Deliverable |
| --- | --- | --- |
| 1 | [#568](https://github.com/clickbytes/minimium/issues/568) | Contract and source inventory |
| 2 | [#569](https://github.com/clickbytes/minimium/issues/569) | Composed operation catalog and help |
| 3 | [#570](https://github.com/clickbytes/minimium/issues/570) | Canonical binder and side-effect-free explanation |
| 4 | [#571](https://github.com/clickbytes/minimium/issues/571) | Typed results and provenance |
| 5 | [#572](https://github.com/clickbytes/minimium/issues/572) | Versioned admin correlation |
| 6 | [#573](https://github.com/clickbytes/minimium/issues/573) | Shared clock |
| 7 | [#574](https://github.com/clickbytes/minimium/issues/574) | Camera and startup reset |
| 8 | [#575](https://github.com/clickbytes/minimium/issues/575) | Actor movement policy patches |
| 9 | [#576](https://github.com/clickbytes/minimium/issues/576) | Authoritative jump outcome |
| 10 | [#577](https://github.com/clickbytes/minimium/issues/577) | Partial-line analysis |
| 11 | [#578](https://github.com/clickbytes/minimium/issues/578) | F1 completion and signature editing |
| 12 | [#579](https://github.com/clickbytes/minimium/issues/579) | PL/EN help, history and input limits |
| 13 | [#580](https://github.com/clickbytes/minimium/issues/580) | Session lifecycle and diagnostics |
| 14 | [#581](https://github.com/clickbytes/minimium/issues/581) | Player actions |
| 15 | [#582](https://github.com/clickbytes/minimium/issues/582) | Navigation and transfer context |
| 16 | [#583](https://github.com/clickbytes/minimium/issues/583) | Local input and presentation |
| 17 | [#584](https://github.com/clickbytes/minimium/issues/584) | Cosmetic meteors and audio |
| 18 | [#585](https://github.com/clickbytes/minimium/issues/585) | Server configuration and administration |
| 19 | [#586](https://github.com/clickbytes/minimium/issues/586) | Primary generation and paged reads |
| 20 | [#587](https://github.com/clickbytes/minimium/issues/587) | Targeted bolids and impacts |
| 21 | [#588](https://github.com/clickbytes/minimium/issues/588) | Component-aware entity removal |
| 22 | [#589](https://github.com/clickbytes/minimium/issues/589) | Committed and pending hydrology |
| 23 | [#590](https://github.com/clickbytes/minimium/issues/590) | Received, requested and installed water presentation |
| 24 | [#591](https://github.com/clickbytes/minimium/issues/591) | All-world persistence jobs |
| 25 | [#592](https://github.com/clickbytes/minimium/issues/592) | Domain-only clock extension |
| 26 | [#593](https://github.com/clickbytes/minimium/issues/593) | Bounded Rhai worker |
| 27 | [#594](https://github.com/clickbytes/minimium/issues/594) | Conditional camera restore and server wait |
| 28 | [#595](https://github.com/clickbytes/minimium/issues/595) | AST cache and costs |
| 29 | [#596](https://github.com/clickbytes/minimium/issues/596) | Inventory reconciliation |
| 30 | [#597](https://github.com/clickbytes/minimium/issues/597) | Integrated static acceptance |
