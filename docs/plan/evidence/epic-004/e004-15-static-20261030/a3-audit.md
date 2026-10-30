# A3 E004-15 handoff — astronomy, geography and pinned data

- Baseline: `93b1264502e33f01a9769901fbbc72697bf904e0`.
- Commit: `2e9790402f9156f9e74f5689f6a423743a45867f`.
- Branch/worktree: `work/e004-15-a3-astronomy`, `D:/Git/minimium-e004-a3`.
- Scope: E004-15 only. **RUNTIME NOT RUN**; no tests, test suites, game, loadtest, independent astronomy reference run, or importer regeneration run.
- Final Git status: clean. Git emits `warning: unable to find all commit-graph files`; this is repository metadata, not a code check failure.
- No vendor content, pin, manifest, generated binding, model revision, wire or save format change.

## Changed files and confirmed defects

1. `crates/astronomy/src/ffi/mod.rs`: process-wide `Mutex<()>` around all four bridge functions, acquired exactly once per call. The private generated bindings have no other Rust callers. Guards cover each unsafe call; no nested wrapper call or Rust callback exists inside the locked C path. `SkyCalculator` retains `PhantomData<Rc<()>>` (`calculator.rs:40`), hence remains !Send/!Sync. Poison recovery takes `PoisonError::into_inner()` and still serializes calls; no Rust invariant is stored in the lock or exposed partially by C, and the C bridge neither calls Rust nor retains pointers.
   - Source defect evidence: vendor `astronomy.c:2148` declares non-atomic `int _CalcMoonCount`, incremented at `:2277`. Separate calculators constructed on separate threads previously could enter concurrently despite each individual calculator being !Send/!Sync.
2. `crates/astronomy/ffi/bridge.c:161` and `bridge.h:18`: transpose the upstream rotation into explicit row-major `[output][input]` bridge storage.
   - Source defect evidence: upstream `Astronomy_RotateVector` at `astronomy.c:10089–10091` computes x from `rot[0][0], rot[1][0], rot[2][0]`, while Rust `frames::matrix_vector` dots each row. The original direct copy silently applied the inverse rotation.
3. `crates/astronomy/src/frames.rs:115–140`, `calculator.rs:101`: distinguish proper EQJ→HOR rotations (determinant +1) from the reflected EQJ→game-world transform (determinant −1), retaining all finiteness, orthogonality and unit-row checks.
   - Source defect evidence: `core/sky/observer.rs` constructs game `N×E=U`; `hor_to_world_matrix` has columns N, −E, U. Thus determinant is −1 for every valid game observer. The previous `validate_rotation` required +1 for the composed matrix and returned `InvalidUpstreamValue` for valid planetary skies. No relaxation to arbitrary handedness was made.
   - A4 cross-check: renderer `adapter::matrix4` accepts finite matrices, transposes rows to WGSL columns, and has no +1 determinant restriction. Stars use w=0 and translation-free sky view/projection; camera applies Earth view reflection once.

## Static ABI and pointer audit

- Compared bridge.h, bindings.generated.rs, and generator SOURCE template field-for-field: `double`↔`f64`, `int32_t`↔`i32`, arrays preserved, `repr(C)`, extern C, output pointer mutability, declaration order and all four signatures match.
- Ephemeris: 15 doubles (120 bytes with 8-byte double alignment on the built MSVC x64 target). Local frame: 13 doubles (104 bytes). ABI offsets/sizes runtime/compile-time assertions remain for E004-16.
- `rtk python scripts/generate_astronomy_bindings.py` reproduces the checked-in logical content (no Git diff); no generated file was hand-edited. Generator is a fixed bridge-ABI template, not a dynamic C parser. ABI version 1 remains appropriate; matrix semantics bug fix changes no layout/signature.
- Rust initializes output structs with Default and scalar outputs with 0.0. C checks pointer non-null, clears struct outputs with memset, validates input range/finiteness, propagates upstream status and rejects nonfinite results. Rust returns an error without publishing partially filled output for nonzero status. C uses only synchronous pointers into the call stack; no retaining/global pointer assignment, callback or allocation ownership transfer.
- `DeltaTFunc` defaults to Espenak/Meeus (`astronomy.c:899`). The bridge exposes no API to mutate that function pointer or backend state. `ASTRONOMY_ENGINE_NO_CURRENT_TIME` is supplied to both C units by build.rs; no host time enters these calculations.

## Units and geography audit

- `time.rs:9`: validate core UTC range, checked integer subtraction from J2000 noon POSIX UTC `946728000000`, then f64 division by `86400000`. No absolute timestamp is converted to f32.
- V1 treats POSIX UTC as backend UT; it does not supply UT1/EOP/leap-second tables. `Astronomy_TimeFromDays` derives TT using the pinned default delta-T model. Independent fixtures must explicitly align these assumptions, especially future dates.
- bridge.c `AU_TO_METERS = KM_PER_AU * 1000.0`; all geocentric/heliocentric and observer positions returned to Rust are meters. Rust subtracts observer EQJ position once for Sun/Moon topocentric direction/distance.
- `core/sky/observer.rs:241` uses Position3Mm feet and a separately supplied eye height. Latitude=`asin(y/r)`, longitude=`atan2(z,x)` normalized to [−π,π), exact poles use 0 longitude. N/E/U formula is explicit; no cube face discontinuity is involved.
- Earth physical eye height is raw-GEBCO physical anchor plus the unscaled eye/residual displacement (`observer.rs:296–301`), not the game radius. Example by formula: raw GEBCO 1000 m, height scale 100‰, game feet 100 m above radius, 1.7 m eye → physical 1001.7 m. Ocean floor uses sea-level anchor, not negative seabed eye elevation. EarthLike explicitly uses radial height and the mapped Earth sky model. Physical height limits −12000..100000 m remain checked.
- Geographic angles enter C in degrees. C HOR is N/W/U; Rust azimuth is atan2(−west,north), radians, [0,2π), with deterministic zenith/nadir azimuth 0. Refraction receives/returns degrees exactly once, then radians; geometric and apparent directions remain distinct.
- RA validation in importer is hours [0,24), declination degrees [−90,90]. The catalogue stores already-equatorial HYG Cartesian positions in pc and velocities in pc/Julian year, so no RA-hours conversion is needed for rendering. Star proper motion uses TT-days /365.25 and normalizes after linear space motion.
- Moon geometry computes normalized moon→Sun and moon→observer vectors; fraction=(1+dot)/2 and angular **radius**=asin(1737400 m/distance). Geocentric solar direction is kept separate for globe lighting. Geometric means without local refraction; it does not reverse the upstream Sun aberration/light-time model.

## Bounded cache audit

- `ephemeris_cache.rs:12,83,133`: exactly two endpoints for the current 300000 ms UTC bracket; reuse matching previous endpoints, otherwise compute at most two. There is no step-replay loop for a 2000→2100 jump. End-of-range upper endpoint is clamped to the last allowed millisecond.
- Clock revision, world instance/content/membership context and observer-model revision invalidate the pair. The local frame/parallax/diurnal rotation are recalculated for the current observer/time each evaluation; teleporting within the same context does not require recomputing geocentric ephemerides.
- `stars.rs:34,64`: at most 500 records; re-evaluate on UTC day change, backward time, forward discontinuity >one day, changed records, or invalidation. The returned directions are Arc-backed; no missed-day replay. Shared calculator context reset invalidates both caches; returning to the world with a new membership key resets appropriately.
- Only angle phase is interpolated through sin/cos to avoid wrap; moon pole is renormalized; directions come from interpolated physical EQJ vectors. Numerical tolerances remain unproven until V11/V12.

## Data provenance, checkout issue and license evidence

All below Git HEAD blob hashes match their pins. A **fresh Windows checkout initially did not**: autocrlf converted pinned text to CRLF. A1 is adding root .gitattributes rules. A3 restored only the original LF bytes locally and refreshed the Git stat cache with `git add --renormalize`; there is no vendor/license/binding content diff.

Suggested exact LF rules:

- `third_party/astronomy-engine/LICENSE`
- `third_party/astronomy-engine/source/c/astronomy.c`
- `third_party/astronomy-engine/source/c/astronomy.h`
- `crates/assets/data/sky/HYG-LICENSE-NOTICE.txt`
- `crates/assets/data/sky/LICENSE-CC-BY-SA-4.0.txt`
- `crates/assets/data/sky/ATTRIBUTION.md`
- `crates/assets/data/sky/stars-v1.manifest.json`
- `crates/assets/src/sky_catalog/identity.generated.rs`
- `crates/astronomy/src/ffi/bindings.generated.rs`

| Artifact | Verified SHA-256 |
| --- | --- |
| Astronomy Engine LICENSE | `690dd98cb13ba4db77c6327deea852a816892bb9debbad5943405c66972f8023` |
| astronomy.c | `388920479d819713963ffd0f8ab0677944f66c6c94239ceddddd3b0d5d064432` |
| astronomy.h | `83a31011957c2b87e22f0828820f370f1a495aa4d51357bb57d714cde839dc00` |
| stars-v1.bin | `81c8d6e0d01455f3416ab40b671831886e8b6dc60b872f504dbc85621dbee3db` |
| stars payload | `846c3194af1d5f3cff83a5e8ff30537aed6bb1145490e61b2c4dcc654a5995d6` |
| HYG source CSV | `d9f69fd86bbf90a4e4d52b4c5c53eacfa6dfc0bfdef85bfd94f095e0bebe4ebd` |
| HYG source license notice | `f404190403d31e0ce7223f4cb7af954485ad88077358330754dc5c892a856627` |
| CC BY-SA 4.0 text | `28a9529c7d0bb4dc51f4bf5c116a3d16ef247a052f7591466768ddf563fd1cf5` |

- Pinned Astronomy Engine: v2.1.19, commit `61dc07020aaa6885d2c7f688a4d82beaf6edb9ef`, annotated tag `03084ee684bdcc490273fe85f9df4f1c8fb66199`, MIT license present.
- Pinned HYG: commit `c7f7f883fe678cc7680169a50ccd7dcc49b060ce`, source CSV 33932548 bytes, Git blob `ba2dec4eb0f6768914c7fc1051258100214ddf84`. Existing local source at `C:/Users/ppotepa/AppData/Local/Temp/hygdata_v41.csv` was hashed directly and matches.
- Direct binary inspection: 32096 bytes, 96-byte header + 500×64 bytes, header count=500, 500 unique IDs, exact ordered IDs equal manifest. Payload/header/manifest hashes agree.
- Static importer inspection: global top 500 by `(Decimal(apparent V mag), HYG id)`, excludes id=0; bounded 64 MiB/200000 rows/16KiB records. Uses source pin checks before parsing and does not choose by visibility. Source manifest historical import counts 119626/119625 and threshold3.96 are not newly regenerated during this stage.
- CC BY-SA 4.0 full text and upstream notice accompany derived asset, ATTRIBUTION names David Nash/Astronexus, source pin and transformations. Code/backend license remains separate.
- `rtk git -c safe.directory=D:/Git/minimium-e004-a3 ls-files third_party/astronomy-engine crates/assets/data/sky` confirms all 5 vendor/provenance files and 5 asset/license files are tracked despite /third_party/ ignore.
- Read-only inspection helper (not a repository test): `D:/Git/minimium-e004-a3-assets-audit.py`. It reads raw hashes/header/IDs only; after LF restoration every reported matches_* value is true. Importer reconstruction remains V11/V12-stage work.

## Executed static controls and actual toolchain

All shell commands started with `rtk`. CARGO_TARGET_DIR was initially `D:/Git/minimium-e004-a3/target-a3`, then moved to ignored `D:/Git/minimium-e004-a3/target/a3` after verifying both absolute paths remain under this worktree. No process was stopped.

| Command | Result |
| --- | --- |
| `rtk cargo fmt --all -- --check` | Initial failure was only lock-expression formatting; `rtk cargo fmt -p minimium-astronomy` fixed it. Final PASS, exit0. |
| `rtk cargo check -p minimium-astronomy --locked` | PASS, exit0. Initial isolated compile 5 crates, final repeat after exact LF vendor restore one crate with one Windows incremental hard-link/copy fallback warning. No Rust diagnostic errors. |
| `rtk cargo clippy -p minimium-astronomy --locked -- -D warnings` | PASS, exit0, no lint diagnostics. |
| `rtk cargo build -p minimium-astronomy --release --locked` | PASS, exit0, C compiled and archive built; final repeat on committed code/LF originals PASS. |
| `rtk cargo build -p minimium-astronomy --release --locked -vv` with CC_ENABLE_DEBUG_OUTPUT=1 | PASS, exit0; raw compiler discovery log in `D:/Git/minimium-e004-a3-build.log`. |
| `rtk python scripts/generate_astronomy_bindings.py` | PASS, exit0, no logical Git diff. |
| `rtk python scripts/check_file_lengths.py` | PASS, exit0, 248 source files, max500 physical lines. |
| `rtk git -c safe.directory=D:/Git/minimium-e004-a3 diff --check` | PASS, exit0. |
| `rtk python D:/Git/minimium-e004-a3-assets-audit.py` | Exit0, final source/vendor/license/binary/hash/ordered-ID inspection all match. |
| repo-check.ps1 immediately before commit | PASS author and HTTPS Git transport confirmed as clickbytes. Used process-local GIT_CONFIG safe.directory setting only; no account/identity/persistent Git configuration changed. |

Actual host: Microsoft Windows NT10.0.19044.0, x86_64-pc-windows-msvc. rustc1.99.0 `(b940084d7 2026-09-28)`, LLVM23.1.1. Actual cc-discovered compiler: `D:/Apps/VC/Tools/MSVC/14.51.36231/bin/HostX64/x64/cl.exe`; file version19.51.36260.0, product14.51.36260.0. cc used `/MD /O2 /Brepro /W4`, defined ASTRONOMY_ENGINE_NO_CURRENT_TIME, and archived with the corresponding lib.exe. `cl.exe` absence from PATH is not evidence of an unavailable toolchain.

Documentation guidance: Windows MSVC Build Tools/Windows SDK or compatible clang-cl with SDK/linker; Linux GCC or Clang plus normal linker/toolchain (`libm` linked by build.rs). Runtime bindgen/libclang not required. **Only Windows/MSVC was actually built here**; Linux and clang-cl are documented procedures, not measured verification.

## Deferred verification / remaining integration action

- A1 must include LF checkout attributes and complete workspace-wide integrated controls, then record actual integration SHA.
- V11: independently verify corrected transforms, both hemispheres/poles/antimeridian/cube boundaries, geometric/apparent directions, lunar fraction/orientation/angular radius, UTC/UT/TT assumptions, catalogue reconstruction.
- V12: direct-vs-cache endpoints/midpoints/±1ms, time scales, context/teleport/reconnect/jumps and counters. Add parallel independently constructed calculators and ABI controls only after phase authorization.
- V13–V17: all celestial orientation and lighting outputs depend on the corrected transforms; historical pre-fix captures cannot accept these scenarios. V18 lifecycle, V20 cost should use final integrated build including mutex.
- No open static astronomy defect remains after A1 applies the checkout fix. Accuracy, concurrency runtime, GPU and visual behavior are **NOT RUN**, not PASS.
