# Epic 002 review and deferred evidence

The following records integration and static review, not a runtime acceptance claim. Full task details, decisions and warnings are in the linked issues and epic matrix. All task PRs target the epic branch.

| Task | Issue | PR | Merge SHA | Remaining evidence |
| --- | --- | --- | --- | --- |
| E002-00 | #15 | #13, #32 | a3cba16, d1ce07 | Baseline runtime scenarios |
| E002-01 | #16 | #33 | 9200bd5 | Identity round trips and reconnect |
| E002-02 | #17 | #34 | 62075a8 | Wire compatibility and malformed frames |
| E002-03 | #18 | #35 | a6873f | Filesystem, locks and endpoint lifecycle |
| E002-04 | #19 | #36 | 10185d | Map paths, geometry and fingerprint |
| E002-05 | #20 | #37 | caafe2 | Physics and lane equivalence |
| E002-06 | #21 | #38 | 54827b | Tick ordering and shutdown |
| E002-07 | #22 | #39 | 186752 | EOF, partial writes, backpressure |
| E002-08 | #23 | #40 | 815db0 | Admission and acknowledgement ordering |
| E002-09 | #24 | #41 | a698e1 | Migration, save failures and recovery |
| E002-10 | #25 | #42 | 2617c2 | DNS, cancellation and process ownership |
| E002-11 | #26 | #43 | ec9cbc | Visuals, input, prediction and interpolation |
| E002-12 | #27 | #44 | 7877b4 | CLI parsing, aliases and help |
| E002-13 | #28 | #45 | 6eb6816 | Token isolation, reports and load measurement |

## Contract evidence matrix

| Contract | Principal files | Static evidence | Deferred scenario |
| --- | --- | --- | --- |
| Protocol v8 and unknown error codes | crates/protocol/src | Explicit codec and discriminants | Byte compatibility, incomplete and invalid frames |
| Profile format and player identity | crates/core/src/identity.rs, crates/platform/src/identity_store.rs | Domain identity separated from file operations | Reconnect and concurrent creation |
| Snapshot v3 and v2 migration | apps/server/src/storage | Versioned decoding and atomic file replacement | Migration, interrupted writes, disk failures |
| Map fingerprint and geometry | crates/assets/src, crates/core/src/world | Shared loader and domain fingerprint | Existing maps, OBJ and collision scenarios |
| Commands and configuration | crates/platform/src/commands.rs, apps/client/src/client_commands.rs | Typed catalogs and documented options | Help, aliases, invalid and duplicate flags |
| Ordered world input and ownership | apps/server/src/runtime.rs, apps/server/src/runtime/inputs.rs, apps/server/src/world_inputs.rs | Runtime owns world and applies queue | Two clients, ordering, acknowledgements |
| Lifecycle and transport | apps/server/src, apps/client/src/connector.rs | Bounded I/O and owned/attached cleanup paths | Timeouts, backpressure, EOF and process ownership |
| Telemetry and report v2 | crates/telemetry/src, apps/loadtest/src/report | Fixed buckets, separate data and writer, token validation | 18 manifests, report output and lane comparison |

The static audit finds no filesystem, network, process, platform-specific, embedded game asset or unsafe references in core using scoped source search. Unsafe code remains in platform's random generation and atomic replace and telemetry's allocator and process sampling. The two existing `large_enum_variant` warnings remain: boxing them would introduce hot-path indirection and requires measurement. Static search and compilation cannot prove that all public APIs are used or that unsafe code is correct.
