# Epic 004 — audyt integracji E004-15

Etap [#67](https://github.com/clickbytes/minimium/issues/67), stan: **IN AUDIT**.
Baseline oraz head epica przy rozpoczęciu: `93b1264502e33f01a9769901fbbc72697bf904e0`.
`origin/main`: `30ff278f9a0020f93d2567b2ca5c05a290bc605e`.
Fetch nie wykazał nowych zmian epica względem planu. Główny checkout i istniejący
archiwalny worktree były czyste. A2–A4 zaczęli w oddzielnych worktree od tego samego SHA.
Repozytorium nie ma `.codegraph`; audyt używał scoped `rg` i odczytów źródeł.
Kontrola tożsamości potwierdziła autora i transport GitHub CLI jako `clickbytes`.

**RUNTIME NOT RUN.** Nie tworzono zestawów testów ani nie uruchamiano testów,
gry, loadtestu, GPU QA ani benchmarków. E004-16 wymaga osobnej zgody użytkownika.
E002/E003 i historyczne przyrosty E004 nie zaliczają tej fazy.

## Mapa dostarczonych kontraktów

Merge SHA odczytano z metadanych GitHub PR #71–86; tabela wiąże je z bieżącymi
źródłami, nie tylko zamknięciem issues. Szczegóły przeglądu domen poniżej.

| Zadanie | PR i rzeczywisty merge SHA | Implementacja kontraktu | Pozostały odbiór |
| --- | --- | --- | --- |
| E004-00 / [#52](https://github.com/clickbytes/minimium/issues/52) | [#71](https://github.com/clickbytes/minimium/pull/71) — `12f84a7a2908a6c8a6006400837f94a18cfcf550`<br>[#72](https://github.com/clickbytes/minimium/pull/72) — `be690c102eb8c75949b3576473827589dde20744` | ADR 0006; `docs/plan/epic-004-contracts.md` | V01–V20: NOT RUN |
| E004-01 / [#53](https://github.com/clickbytes/minimium/issues/53) | [#73](https://github.com/clickbytes/minimium/pull/73) — `ad09c5cdaf9b8769c7be197d1dc2aebab9f3c9be` | `crates/core/src/time{.rs,/types.rs,/evaluation.rs}`; `environment.rs` | V01–V02, V08: NOT RUN |
| E004-02 / [#54](https://github.com/clickbytes/minimium/issues/54) | [#74](https://github.com/clickbytes/minimium/pull/74) — `1be068932d32afd258612c4a3a7b50ad26afb8ce` | `apps/server/src/environment/clock.rs`; `runtime/operations.rs`; `crates/platform/src/commands/time.rs` | V01–V02, V19: NOT RUN |
| E004-03 / [#55](https://github.com/clickbytes/minimium/issues/55) | [#75](https://github.com/clickbytes/minimium/pull/75) — `77218a6e09ba1dedeba106dda3819c7c9eaeee06` | `apps/server/src/storage/{format,player_store,persistence}.rs`; `startup.rs`; `lifecycle.rs` | V08–V10: NOT RUN |
| E004-04 / [#59](https://github.com/clickbytes/minimium/issues/59) | [#79](https://github.com/clickbytes/minimium/pull/79) — `a69ff3373af810a1bc72a91c77c2a483486fcfe6` | `crates/protocol/src/{models,encoding,decoding,limits}.rs`; server runtime; desktop/text session; loadtest | V03–V07: NOT RUN |
| E004-05 / [#60](https://github.com/clickbytes/minimium/issues/60) | [#80](https://github.com/clickbytes/minimium/pull/80) — `c40db94560a0490aa7159b13ae58fb053f88a930` | `apps/client/src/session/{clock,environment,receive}.rs`; `environment/clock.rs`; connection lifecycle | V04–V07, V19: NOT RUN |
| E004-06 / [#56](https://github.com/clickbytes/minimium/issues/56) | [#76](https://github.com/clickbytes/minimium/pull/76) — `dfbc7958bf645a34000514335c6d96c095f9ce8e` | `crates/core/src/sky`; `apps/client/src/renderer/environment/observer.rs`; `crates/procgen/src/earth.rs` | V11–V12, V16: NOT RUN |
| E004-07 / [#57](https://github.com/clickbytes/minimium/issues/57) | [#77](https://github.com/clickbytes/minimium/pull/77) — `69a633e7da7df40016eab1043196e44370669f35` | `crates/astronomy/{build.rs,ffi,src}`; tracked `third_party/astronomy-engine` | V11–V12; FFI concurrency: NOT RUN |
| E004-08 / [#58](https://github.com/clickbytes/minimium/issues/58) | [#78](https://github.com/clickbytes/minimium/pull/78) — `cb3d70a08abdcd7bee5d6eccbd2ba05eb0e7c066` | `crates/assets/data/sky`; `crates/assets/src/sky_catalog`; `scripts/import_hyg_stars.py` | V11, V14; exact reproduction: NOT RUN |
| E004-09 / [#61](https://github.com/clickbytes/minimium/issues/61) | [#81](https://github.com/clickbytes/minimium/pull/81) — `3155a2c08af09d1e266a45965b9768859baeb026` | `apps/client/src/renderer/{environment,environment_gpu,geometry}.rs`; GPU/frame WGSL | V13–V18: NOT RUN |
| E004-10 / [#62](https://github.com/clickbytes/minimium/issues/62) | [#82](https://github.com/clickbytes/minimium/pull/82) — `45b4f298fd53420a6bb2039fe2d1a0d04201eb36` | renderer sky/Sun; world/globe shaders; linear output composition | V13, V16: NOT RUN |
| E004-11 / [#63](https://github.com/clickbytes/minimium/issues/63) | [#83](https://github.com/clickbytes/minimium/pull/83) — `325ab46872defeca70c05529b9a9f490a7dd80f9` | renderer stars; `stars.wgsl`; 500-instance buffer | V14: NOT RUN |
| E004-12 / [#64](https://github.com/clickbytes/minimium/issues/64) | [#84](https://github.com/clickbytes/minimium/pull/84) — `4750485993f1602d030fca757ed103f7c731d0e3` | renderer Moon; `moon.wgsl`; shared local/global lighting | V15–V16: NOT RUN |
| E004-13 / [#65](https://github.com/clickbytes/minimium/issues/65) | [#85](https://github.com/clickbytes/minimium/pull/85) — `a27e79f2c1f92d0a973e5fb503355aac7b15194d` | renderer shadow/caster selection, depth pass and world sampling | V17–V18: NOT RUN |
| E004-14 / [#66](https://github.com/clickbytes/minimium/issues/66) | [#86](https://github.com/clickbytes/minimium/pull/86) — `93b1264502e33f01a9769901fbbc72697bf904e0` | client quality/diagnostics/stats; server sync counters | V18–V20: NOT RUN |

## Granice zależności i dokumentacja

`scripts/check_architecture.py` pobiera pełne `cargo metadata --locked --all-features --format-version 1`, z rozwiązaną tożsamością pakietów. Przechodzi
wyłącznie krawędzie normal/build (także zewnętrzne i warunkowe dla platform);
dev-dependencies nie są ścieżką produkcyjną. Dla core/protocol/server/ctl zabrania
astronomy, wgpu, wgpu-core, wgpu-hal i winit, zachowując wcześniejsze granice
core/assets. Błąd pokazuje pełną ścieżkę i rodzaj krawędzi. Nie używa
`--no-deps` ani `--offline`; graf nie stanowi dowodu braku I/O w kodzie.

README opisuje C toolchain i konieczność budowania całego workspace przed
lokalnym klientem. Architektura opisuje crate astronomii, v12, aktualny rejestr
poleceń i uniformy camera/environment/shadow 208/320/128 B. ADR 0006 oddziela
historyczny brak `cl.exe` w PATH od wykonanej kompilacji Windows.

## Weryfikacja końcowa

## Wyniki przeglądu domen

Pełne raporty źródłowe i dokładne kontrole wykonane przed integracją:
[A2 — czas/sieć/zapis](evidence/epic-004/e004-15-static-20261030/a2-audit.md),
[A3 — astronomia/FFI/dane](evidence/epic-004/e004-15-static-20261030/a3-audit.md),
[A4 — renderer/metryki](evidence/epic-004/e004-15-static-20261030/a4-audit.md).
Raporty zachowują SHA gałęzi roboczych; cherry-picki integracyjne to odpowiednio
`2faa178`, `190fa3e`, `3e39b4f`. Reguły LF wymagane przez raport A3 są już w
`.gitattributes`; vendor, piny i wygenerowane assety nie mają zmian zawartości.

| Defekt | Właściciel i poprawka | Statyczny wynik / późniejszy odbiór |
| --- | --- | --- |
| Niezależne kalkulatory mogą równolegle zmieniać `_CalcMoonCount` C | A3: jeden procesowy mutex, cztery wejścia, poison recovery, zachowane !Send/!Sync | Naprawiony w źródle; concurrency/ABI V11–V12 NOT RUN. |
| C `rot[input][output]` kopiowane jak rustowe wiersze | A3: transpozycja na bridge | Naprawiony; V11–V17 wymagają odbioru poprawionej orientacji. |
| Poprawna ramka world z determinant −1 odrzucana jako zła rotacja | A3: osobna walidacja rotacji +1 i transformacji world −1 | Naprawiony; bez rozluźniania ortonormalności/finite; V11–V17 NOT RUN. |
| Commit portalu ucina resztę częściowo wysłanej ramki TCP | A2: zachowanie bufora, offsetu i inflight Ping/probe | Naprawiony; stare wejścia świata nadal odrzucane przez context; V03/V06/V07/V19 NOT RUN. |
| Portal bez limitu kolejki, teleport bez rezerwy na Leave | A2: wspólny limit 4096 z rezerwą liczby klientów | Naprawiony; V03/V07/V19 NOT RUN. |
| Helper `environment` zasłaniany w istniejącym teście | A2: `environment_fixture` | Istniejący test skompilowany przez cargo check --tests, nie uruchomiony; V03 NOT RUN. |
| Przygotowanie i upload łączone w metryce CPU; brak p50/próbek/caster bytes | A4: prepared frame, osobny upload i disjoint shadow scope, diagnostyka v2 | Naprawiony; V18/V20 NOT RUN, GPU elapsed n/a. |
| Windows autocrlf zmienia bajty przypiętych tekstów i ich SHA-256 | A1/A3: dziewięć jawnych reguł LF | Hashe źródeł/blobów zgodne; odtworzenie katalogu pozostaje w #68. |
| Checker nie widzi zewnętrznych zależności przechodnich ani pełnych zakazów | A1: pełny graf Cargo normal/build | Bieżący graf PASS; źródła błędu wskazywane pełną ścieżką. |

Zegar serwera → Welcome/EnvironmentSync → ClientSession → EnvironmentFrameInput
ma spójne POSIX/monotonic ms i ppm; fizyka, cooldowny, heartbeat i autosave pozostają
w realnym czasie. Prepare nie przełącza aktywnego profilu, commit wiąże profil
z nowym context, cancel pozostawia źródło, reconnect odrzuca stary nonce. Snapshot
save v8 przechwytuje zegar na owner thread przed workerem i zachowuje migracje
v2–v7 oraz atomowy zapis. Szczegóły i limity są w raporcie A2.

Jednostki astronomii pozostają f64, UTC→J2000, UT→TT przez przypięty model ΔT,
AU→m i oddzielna fizyczna wysokość oka. Geocentryczne Sun zasila globus,
geometryczne lokalne Sun/Moon światło i cienie, pozorne kierunki tarcze. Cache ma
dwa końce przedziału 300 s bez pętli nadrabiania lat, gwiazdy najwyżej 500 rekordów.
Raport A3 zawiera zmierzone SHA-256 vendor/MIT, HYG/CC BY-SA i 500 unikalnych ID.

Wyszukanie `0\\.46\\s*,\\s*0\\.82\\s*,\\s*0\\.34` w rendererze: brak wyników.
Przegląd world/globe/water nie wykazał dawnego minimalnego światła planetarnego.
Kolory atmosfery, parametry radiancji i ambient Interior są celowym modelem wyglądu.
Raport A4 rozlicza uniformy 208/320/128 B, odbicie Earth, brak translacji gwiazd,
fallback, bounded caster resources oraz pomiary v2. Wygląd i GPU nie są zaliczone.

## Następna bramka

Właściciele, scenariusze i format artefaktów: [ledger](epic-004-verification.md).
Kompletne instrukcje przed przyszłym uruchomieniem: [procedura](epic-004-final-verification.md).
Zamknięcie #67 potwierdza wyłącznie statyczny odbiór integracji; #68 i #69
pozostają osobnymi bramkami.

