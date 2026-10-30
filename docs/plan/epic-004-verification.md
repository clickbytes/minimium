# Epic 004 — plan i rejestr weryfikacji

**Epic:** [#51](https://github.com/clickbytes/minimium/issues/51). **Właściciel finalnego odbioru:** [E004-16 / #68](https://github.com/clickbytes/minimium/issues/68).
**Baza planu:** `aad2c8298606a46dc3a286b4eac7f1de45759851`. **Kodowy baseline E004-00:** `cd816c2d32bd4b1447f14ac04a9ec99e44c0e4af` (integracja `main@30ff278`). **Stan:** E004-00–14 zintegrowane; ostatni PR [#86](https://github.com/clickbytes/minimium/pull/86), merge `93b1264502e33f01a9769901fbbc72697bf904e0`. #67: STATIC PASS na 8a0a33d703ae356fe3774df7cf5461e52d296975; #68 oczekuje na osobną zgodę użytkownika po #67; #69 oczekuje na #68. **RUNTIME NOT RUN.**

[Audyt i rzeczywiste merge SHA wszystkich przyrostów](epic-004-audit.md).
[Kompletna procedura końcowego odbioru, właściciele i artefakty](epic-004-final-verification.md).
Poniższy rejestr przyrostów jest historyczny; końcowe kontrole E004-15 są zapisane
w audycie. Wcześniejsze wyniki E002/E003 nie zaliczają żadnego V-ID E004.

## Historyczny rejestr przyrostów

| Obszar | Stan | Dowód / następny krok |
| --- | --- | --- |
| Kompletność 18 opisów i graf zależności | Przegląd planu wykonany | Cele, pliki, kroki, kontrakty, kryteria, zależności i zakres; graf acykliczny. To nie jest kontrola kodu E004. |
| Referencje do kodu | Sprawdzone | Issues zawierają historyczne ścieżki bazowe do `aad2c82`; aktualny punkt integracji `cd816c2` zachowuje zmiany main `30ff278`, w tym nowsze pliki konsoli i globusa. |
| Rust/C toolchain, target, ostrzeżenia bazowe | CAPTURED | Windows, target `x86_64-pc-windows-msvc`; `rustc 1.99.0 (b940084d7 2026-09-28)`, `cargo 1.99.0 (5f94df478 2026-08-27)`, toolchain z `rust-toolchain.toml`. Clang i GCC dostępne, MSVC `cl.exe` niewykryty; była to obserwacja PATH z E004-00; kompilację C/FFI Windows potwierdzono w E004-07, a bieżący capture zapisuje audyt E004-15. Cargo incremental hard-link warnings na `D:\Git\minimium\target`, fallback kopiowania. `cargo check` 0 errors / 12 warnings; Clippy 0 errors / 11 warnings. |
| Implementacja funkcji | E004-00–14 zintegrowane | Audyt #67 obejmuje pełną implementację; runtime pozostaje NOT RUN. |
| Kompilacja, fmt, clippy, architektura, limity plików | BASELINE PASS | Head `cd816c2`; `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors, 12 warnings), `cargo clippy --workspace --locked` (0 errors, 11 warnings), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (163 pliki, max 500 linii). Nie używano `-D warnings`. To baseline gałęzi epica, nie odbiór późniejszej implementacji. |
| E004-01 statyczne kontrole | PASS na `a8577b4` | `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors; 12 warnings o hard-link cache), `cargo clippy --workspace --locked` (0 errors; 12 ostrzeżeń o hard-link cache, bez lint diagnostics), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (167 plików, max 500 linii), `git diff --check`. Żadne testy ani aplikacja nie były uruchamiane. |
| E004-02 statyczne kontrole | PASS na `c9a6855` | `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors; 11 ostrzeżeń o Windows incremental hard-link fallback), `cargo clippy --workspace --locked` (0 errors; 11 tych samych ostrzeżeń, bez diagnostyk lintów), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (174 pliki, max 500 linii), `git diff --check`. Żadne testy ani aplikacja nie były uruchamiane. |
| E004-03 statyczne kontrole | PASS na `c7d6000` | PR [#75](https://github.com/clickbytes/minimium/pull/75), merge `77218a6`; `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors; 11 ostrzeżeń o Windows incremental hard-link fallback), `cargo clippy --workspace --locked` (0 errors; 11 tych samych ostrzeżeń, bez diagnostyk lintów), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (176 plików, max 500 linii), `git diff --check`. Żadne testy ani aplikacja nie były uruchamiane. |
| E004-06 statyczne kontrole | PASS na `15ac4d2` | PR [#76](https://github.com/clickbytes/minimium/pull/76), branch `feature/e004-06-observer-frame`; `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors; 12 ostrzeżeń o Windows incremental hard-link fallback), `cargo clippy --workspace --locked` (0 errors; 12 tych samych ostrzeżeń, bez diagnostyk lintów), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (181 plików, max 500 linii), `git diff --check`. Żadne testy ani aplikacja nie były uruchamiane. |
| E004-07 statyczne kontrole | PASS na `5da228e` | PR [#77](https://github.com/clickbytes/minimium/pull/77), branch `feature/e004-07-astronomy`; `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors; 13 ostrzeżeń o Windows incremental hard-link fallback), `cargo check --workspace --release --locked`, `cargo build --workspace --release --locked` (statyczne linkowanie Astronomy Engine C na `x86_64-pc-windows-msvc`), `cargo clippy --workspace --locked -- -D warnings` (pass; ostrzeżenia tylko o incremental hard-link fallback), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (196 plików, max 500 linii), `git diff --check`. Brak testów, porównań astronomicznych i uruchomienia aplikacji; final verification pozostaje odroczone do E004-16. |
| E004-08 statyczne kontrole | PASS na `38157c0` | PR [#78](https://github.com/clickbytes/minimium/pull/78), branch `feature/e004-08-star-catalogue`; `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors; 14 ostrzeżeń o Windows incremental hard-link fallback), `cargo clippy --workspace --locked -- -D warnings` (0 lint errors; te same ostrzeżenia), `cargo build --workspace --release --locked` z tymczasowym `CARGO_TARGET_DIR` (pass; standardowy katalog release miał zablokowane pliki client/server), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (204 pliki, max 500 linii), `ruff check`, `python -m compileall`, `git diff --check`. Import pinned HYG: CSV 33 932 548 B, Git blob `ba2dec4e…ddf84`, SHA-256 `d9f69fd8…e4ebd`; 119 626 wierszy, 500 rekordów, asset 32 096 B, SHA-256 `81c8d6e0…e3db`. Żadne testy ani aplikacja nie były uruchamiane; final verification pozostaje odroczone do E004-16. |
| E004-04 implementation | PASS on `7c49d92` (PR [#79](https://github.com/clickbytes/minimium/pull/79)) | `cargo fmt --all -- --check`; `cargo check --workspace --locked` (0 errors, 14 Windows incremental hard-link warnings); `cargo clippy --workspace --locked` (0 errors, 14 same warnings); `cargo build --workspace --release --locked` using isolated temporary `CARGO_TARGET_DIR` (pass); `python scripts/check_architecture.py`; `python scripts/check_file_lengths.py` (206 source files, max 500 lines); `git diff --check`. No tests, applications or load generator were run; runtime verification is deferred to E004-16. |
| E004-05 implementation | PASS on `2d58ac2` (PR [#80](https://github.com/clickbytes/minimium/pull/80)) | `cargo fmt --all -- --check`; `cargo check --workspace --locked` (0 errors; 14 Windows incremental hard-link warnings); `cargo clippy --workspace --locked` (0 errors; 14 Windows incremental hard-link warnings, no lint diagnostics); `cargo build --workspace --release --locked` with isolated temporary `CARGO_TARGET_DIR` (pass; 134 crates compiled); `python scripts/check_architecture.py`; `python scripts/check_file_lengths.py` (212 source files, max 500 lines); `git diff --check`. No tests, applications or load generator were run; runtime verification is deferred to E004-16. |
| E004-05 integration | PASS | PR [#80](https://github.com/clickbytes/minimium/pull/80), merge `c40db94560a0490aa7159b13ae58fb053f88a930`; issue #60 closed with final verification pending. |
| E004-09 static implementation | PASS on `6c19d2e` (PR [#81](https://github.com/clickbytes/minimium/pull/81), merged as `3155a2c`) | `cargo fmt --all -- --check`; `cargo check --workspace --locked` (0 errors; 14 Windows incremental hard-link fallback warnings); `cargo clippy --workspace --locked` (0 errors; 14 same warnings, no lint diagnostics); `cargo build --workspace --release --locked` with isolated temporary `CARGO_TARGET_DIR` (pass; 134 crates compiled); `python scripts/check_architecture.py`; `python scripts/check_file_lengths.py` (219 source files, max 500 lines); `git diff --check`. Runtime validation, tests, app runs and visual QA NOT RUN; all runtime scenarios remain deferred to E004-16. |
| E004-09 integration | PASS | PR [#81](https://github.com/clickbytes/minimium/pull/81), merge `3155a2c08af09d1e266a45965b9768859baeb026`; issue #61 closed with final verification pending. |
| E004-10 static implementation | PASS on `951b8b2` (PR [#82](https://github.com/clickbytes/minimium/pull/82), merge `45b4f29`) | `cargo fmt --all -- --check`; `cargo check --workspace --locked` (0 errors; 14 Windows incremental hard-link fallback warnings); `cargo clippy --workspace --locked -- -D warnings` (pass; same Cargo cache warnings, no lint diagnostics); `cargo build --workspace --release --locked` with isolated temporary `CARGO_TARGET_DIR` (pass; 134 crates compiled on first build, 1 after final adjustment); Naga 30.0.1 parser/validator over world, globe, sky, Sun, marker, UI and output-composition WGSL (all PASS); `python scripts/check_architecture.py`; `python scripts/check_file_lengths.py` (227 source files, max 500 lines); `git diff --check`. GPU pipeline/runtime validation, tests, app runs and visual QA NOT RUN; final scenarios remain deferred to E004-16. |
| E004-10 integration | PASS | PR [#82](https://github.com/clickbytes/minimium/pull/82), merge `45b4f298fd53420a6bb2039fe2d1a0d04201eb36`; issue #62 closed with final verification pending. |
| E004-11 integration | PASS | PR [#83](https://github.com/clickbytes/minimium/pull/83), merge `325ab46872defeca70c05529b9a9f490a7dd80f9`; issue #63 closed with final verification pending. |
| E004-11 static implementation | PASS on `eee83e8` (PR [#83](https://github.com/clickbytes/minimium/pull/83), merge `325ab46`) | `cargo fmt --all -- --check`; `cargo check --workspace --locked` (0 errors; 14 Windows incremental hard-link fallback warnings); `cargo clippy --workspace --locked -- -D warnings` (pass; same Cargo cache warnings, no lint diagnostics); `cargo build --workspace --release --locked` with isolated `CARGO_TARGET_DIR` (pass; 1 crate rebuilt after WGSL correction); Naga 30.0.1 parser/validator over stars plus world, globe, sky, Sun, marker, UI and output-composition WGSL (all PASS); `python scripts/check_architecture.py`; `python scripts/check_file_lengths.py` (229 source files, max 500 lines); `git diff --check`. GPU device pipeline validation, tests, app runs and visual QA NOT RUN; final scenarios remain deferred to E004-16. |
| E004-12 integration | PASS | PR [#84](https://github.com/clickbytes/minimium/pull/84), merge `4750485993f1602d030fca757ed103f7c731d0e3`; issue #64 closed with final verification pending. |
| E004-12 static implementation | PASS on `0d162e1` (PR [#84](https://github.com/clickbytes/minimium/pull/84), merge `4750485`) | `cargo fmt --all -- --check`; `cargo check --workspace --locked` (0 errors; 14 Windows incremental hard-link fallback warnings); `cargo clippy --workspace --locked -- -D warnings` (pass; same Cargo cache warnings, no lint diagnostics); `cargo build --workspace --release --locked` with isolated `CARGO_TARGET_DIR` (pass; 1 crate rebuilt after Moon shader changes); Naga 30.0.1 parser/validator over Moon plus stars and existing renderer WGSL (all PASS); `python scripts/check_architecture.py`; `python scripts/check_file_lengths.py` (232 source files, max 500 lines); `git diff --check`. GPU device pipeline validation, tests, app runs and visual QA NOT RUN; final scenarios remain deferred to E004-16. |
| E004-13 integration | PASS | PR [#85](https://github.com/clickbytes/minimium/pull/85), merge `a27e79f2c1f92d0a973e5fb503355aac7b15194d`; issue #65 closed with final verification pending. |
| E004-13 static implementation | PASS on `f1e0d6b` (PR [#85](https://github.com/clickbytes/minimium/pull/85), merge `a27e79f`) | `cargo fmt --all -- --check`; `cargo check --workspace --locked` (0 errors; 14 Windows incremental hard-link fallback warnings); `cargo clippy --workspace --locked -- -D warnings` (pass; same warnings, no lint diagnostics); `cargo build --workspace --release --locked` with isolated temporary `CARGO_TARGET_DIR` (pass; shared target could not replace a locked `minimium-server.exe`); Naga 30.0.1 parser/validator over the shadow-depth shader and combined world sampling shader (both PASS); `python scripts/check_architecture.py`; `python scripts/check_file_lengths.py` (240 source files, max 500 lines); `git diff --check`. An ancillary `cargo check --workspace --all-targets` did not compile because `crates/protocol/src/tests.rs` shadows its `environment` helper with a local binding (E0618); no test binary or application was run. GPU device pipeline validation and visual QA remain NOT RUN and are deferred to E004-16. |
| E004-14 static implementation | PASS on `cdb649793a390462aed89949eeaf2259f38888af`, PR #86, merge `93b1264502e33f01a9769901fbbc72697bf904e0` | `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors; 14 Windows incremental hard-link fallback warnings), `cargo clippy --workspace --locked -- -D warnings` (0 lint errors; same cache warnings), isolated `cargo build --workspace --release --locked`, `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (248 source files, max 500 lines), and `git diff --check` pass. No tests or app were run; GPU/visual behavior remains NOT RUN and deferred to E004-16. |
| Unit/integration/e2e, aplikacja i loadtest | NOT RUN | Wyraźne rozpoczęcie końcowej fazy przez użytkownika oraz ukończone E004-15. |
| Wizualne QA i astronomiczne referencje | NOT RUN | Nie zastępować wartościami z planu ani wcześniejszymi wynikami E002/E003. |
| Sprzęt historyczny / 500 graczy | NOT MEASURED | Raportować dostępność platformy i realne pomiary; nie deklarować na podstawie planu. |

## Wypełniany ledger scenariuszy

| Scenariusz / właściciel | Wynik | Head SHA | Środowisko / wejścia | Dowód / defekt |
| --- | --- | --- | --- | --- |
| V01 / A2 | NOT RUN | — | — | — |
| V02 / A2 | NOT RUN | — | — | — |
| V03 / A2 | NOT RUN | — | — | — |
| V04 / A2 | NOT RUN | — | — | — |
| V05 / A2 | NOT RUN | — | — | — |
| V06 / A2 | NOT RUN | — | — | — |
| V07 / A2 + A4 | NOT RUN | — | — | — |
| V08 / A2 | NOT RUN | — | — | — |
| V09 / A2 | NOT RUN | — | — | — |
| V10 / A2 | NOT RUN | — | — | — |
| V11 / A3 | NOT RUN | — | — | — |
| V12 / A3 | NOT RUN | — | — | — |
| V13 / A4 | NOT RUN | — | — | — |
| V14 / A4 | NOT RUN | — | — | — |
| V15 / A4 | NOT RUN | — | — | — |
| V16 / A4 | NOT RUN | — | — | — |
| V17 / A4 | NOT RUN | — | — | — |
| V18 / A4 | NOT RUN | — | — | — |
| V19 / A2 + A4 | NOT RUN | — | — | — |
| V20 / A2 + A4 | NOT RUN | — | — | — |

Wyniki dopisywać z poleceniem, datą, rzeczywistym integration SHA, parametrami i trwałym artefaktem.
Dopuszczalne stany: PASS, FAIL, BLOCKED, NOT RUN. N/A wymaga konkretnego uzasadnienia; nie wyłącza obowiązkowej funkcji.
Oddzielić dowody statyczne od runtime. Każda poprawka po pomiarze wskazuje, które scenariusze wymagają ponowienia.

## Cel i warunek rozpoczęcia

Sprawdzić działanie całego E004 na zintegrowanym kodzie i zapisać reprodukowalne dowody. **Status początkowy: BLOCKED — oczekuje na ukończenie E004-15 oraz wyraźne polecenie użytkownika rozpoczęcia końcowej fazy testowej.** Utworzenie tego issue, przygotowanie epica ani wcześniejsze zgody dotyczące E002/E003 nie oznaczają uruchomienia nowej fazy testowej.

Zgodnie z [AGENTS.md](https://github.com/clickbytes/minimium/blob/aad2c8298606a46dc3a286b4eac7f1de45759851/AGENTS.md), dopiero w tej fazie tworzyć i uruchamiać test suites, testy integracyjne/e2e, aplikację i obciążenie potrzebne do poniższych scenariuszy. Plan można uzupełniać wcześniej, ale wszystkie wyniki pozostają `NOT RUN`.

## Pliki i artefakty

- `docs/plan/epic-004-verification.md` — główny ledger: scenario ID, head SHA, data, platforma/CPU/GPU/driver, build, seed/preset, współrzędne, UTC, skala, polecenie, wynik, link do artefaktu.
- Nowe `crates/astronomy/tests/{reference,coordinates,interpolation}.rs` — dopiero w autoryzowanej fazie, jeśli wybrany test wymaga tego poziomu.
- Nowe/rozszerzone testy w `core/time`, `protocol`, `server/storage` oraz `client/environment`; wydzielić moduły zgodnie z limitem 500.
- `tests/fixtures/astronomy` lub jedna udokumentowana lokalizacja fixture w crate astronomii: zamrożone referencje JPL/USNO/HYG, źródło, dokładne parametry zapytania, data pobrania, jednostki i hash.
- `docs/plan/evidence/epic-004` — małe raporty tekst/JSON, manifest screenów i capture; większe obrazy zgodnie z polityką repo, z trwałym linkiem. Żadnych tokenów ani prywatnych katalogów użytkownika.

## Przygotowanie referencji i tolerancje

1. JPL Horizons dla Sun/Moon: jawny Earth observer, geodezyjne lat/lon wschodnio-dodatnie i fizyczna wysokość; zapisać oznaczenie skali czasu, frame i wariant refrakcji oraz założenia ΔT/DUT1/EOP i przyszłych sekund przestępnych, szczególnie dla 2050/2100. Porównywać zgodne warianty, nie geocentric z topocentric lub refracted z airless.
2. Referencje obejmują `2000-01-01`, daty 2026, `2050-01-01` i końcówkę 2100; równonoce/przesilenia liczone dla wybranego roku. Nie zgadywać godzin pełni: wybrać rzeczywiste fazy z referencji i zapisać ich UTC.
3. Miejsca: Bergen `60.4N,5.3E`, Warszawa `52.2297N,21.0122E`, równik `0,0`, Sydney `33.8688S,151.2093E`, Tromsø `69.6492N,18.9553E`, analogiczne wysokie szerokości południowe, punkty obu stron ±180°, geograficzne bieguny i pary po obu stronach szwów cube-sphere.
4. Dla dokładnie zadanego UTC/obserwatora, geometryczna separacja kierunków Sun/Moon względem referencji **<=0.05°** przy wysokości >5°. Liczyć kąt między wektorami; sam błąd azymutu nie jest sensowny w zenicie.
5. `abs(illuminated_fraction-reference)<=0.002`, błąd rozmiaru kątowego Księżyca <=1% (gdy referencja podaje średnicę, porównać ją z `2*angular_radius_rad`), błąd kąta pozycyjnego terminatora <=1° dla dobrze określonej fazy (fraction 0.1..0.9, z dala od osobliwości). Kąt pozycyjny porównać po tej samej transformacji celestial north → baza lokalna → finalny widok; sprawdzić kierunek jasnej strony, aby nie zaakceptować odwrócenia o 180°. Przy nowiu/pełni nie stosować nieokreślonego kąta; ocenić poprawne zasłanianie.
6. Gwiazdy: dokładnie 500 stabilnych ID, odtwarzalny hash; dla tego samego modelu pozycji/precesji ruchome kierunki <=0.1° względem niezależnej referencji. Uwzględnić RA w godzinach, pmRA i epokę, a poprawki pomijane w MVP jasno zapisać.
7. Przy horyzoncie Sun/Moon porównywać z tą samą standardową refrakcją; gwiazdy V1 porównywać bez refrakcji zgodnie z ich jawnym uproszczeniem; nie obiecywać uniwersalnej dokładności sekundy wschodu wobec rzeczywistej atmosfery i gór. Obowiązkowo sprawdzić brak NaN, skoków i zapętleń.
8. Tolerancje powyżej dotyczą matematyki dla podanego UTC. Błąd oszacowania czasu sieciowego mierzyć i raportować osobno, z przeliczeniem przez skalę.

Źródła referencyjne: [JPL Horizons manual](https://ssd.jpl.nasa.gov/horizons/manual.html), [USNO sidereal time](https://aa.usno.navy.mil/data/siderealtime), [USNO rise/set/twilight](https://aa.usno.navy.mil/faq/RST_defs). Fixture nie może być wygenerowane wyłącznie biblioteką badaną tym samym testem.

## Macierz obowiązkowa

| ID | Przypadek i spodziewany wynik |
| --- | --- |
| V01 | CoreClock: 0/0.1/1/60/240/1440/1000000x, zmiany skali, set date, RealTime vs TimeScale1, granice dat, overflow i niepoprawne ppm; brak zależności od liczby ticków. |
| V02 | Server: pominięte ticki i długie przerwy, brak graczy, korekty SystemTime w obie strony; fizyka/cooldown/timeout/autosave nie otrzymują skali. |
| V03 | Wire: round-trip wszystkich nowych ramek, maksymalne długości i ucięcie, niewłaściwe tagi/nonce/id/revision/context, wersja12↔13, rate limit; v13 obsługuje wszystkie aplikacje. |
| V04 | Dwa klienty z innym zegarem systemowym: ta sama próbka świata i niezależna lokalizacja; 4-probe bootstrap, pojedyncze inflight i okres 5 s, brak nieograniczonego ruchu. |
| V05 | Kontrolowane symetryczne RTT 0/50/200ms i jitter; po stabilizacji (8 ważnych próbek) oszacowanie monotonicznego offsetu w granicy max(10ms, połowa mierzonego jitteru). Test asymetrii raportuje niepewność, nie udaje gwarancji niemożliwej bez zewnętrznej synchronizacji. |
| V06 | Reconnect i restart server nonce, spóźnione odpowiedzi, stary context/revision; nie wraca stara data ani opis świata. |
| V07 | Planet→dungeon→planet: prepare/cancel/ready/commit, brak przełączania sky przed commit, czas biegnie w Interior; teleport i zmiana render_origin nie odbijają osi. |
| V08 | Save8: manual/autosave/final save, pause/advance offline, clock backward, endpointy zakresu; opóźnienie workera nie zmienia daty snapshotu. |
| V09 | Migracja wszystkich aktualnie obsługiwanych save v2–v7 z zachowaniem ich własnych reguł fingerprint/manifest; dane niezgodne nadal odrzucone, brak resetu danych przy błędzie. |
| V10 | Atomowość: błąd zapisu, za duży snapshot, nieznane pola/duplikaty zegara; poprzedni poprawny zapis pozostaje dostępny. |
| V11 | Astronomia: miejsca i daty z referencji, obu półkul, precesja/ruch własny, granica±180°, bieguny/zenit; geometryczna i pozorna pozycja rozróżnione. |
| V12 | Interpolacja: porównanie direct backend vs cache przy skali0..1000000 i skoku lat; błąd interpolacji kierunku <=0.001° względem bezpośredniego backendu, lokalne obroty co klatkę, brak odtwarzania pominiętych przedziałów. |
| V13 | Niebo: wschód/zachód we właściwym kierunku, zmierzchy i polar day/night, brak sztucznej zmiany o stałej godzinie; globus i FPP mają zgodny kierunek dnia. |
| V14 | Gwiazdy: low i standard mają te same 500 ID/kierunki; jeden batch; horyzont/teren zasłania; brak punktów przez planetę i ciemną tarczę Księżyca. |
| V15 | Księżyc: rzeczywiste pełnia/nów/kwadry/sierpy, orientacja obu półkul, widoczność w dzień, angular size, brak czarnego wycięcia w jasnym niebie. |
| V16 | Światło: normalne cube i terenu, lokalne/globalne światło, bez jasnego oceanu nocą; Interior nie dziedziczy przypadkowo światła dziennego planety. |
| V17 | Shadows: obiekt poza frustum kamery rzuca cień do widoku, niskie Słońce, Sun→Moon→none, border fade/bias/acne, rebase/teleport; incomplete caster coverage sygnalizowane. |
| V18 | Lifecycle: resize, minimalizacja długa, skipped surface, utrata/recreate zasobów, quality low↔standard wielokrotnie; brak zatrzymanego wycieku i wzrostu liczby zasobów. |
| V19 | Komendy: help/completion, błędy, readonly klienta, mutacja wyłącznie przez admin; owned/attached local server pozostaje poprawne. |
| V20 | Pomiar kosztu: fixed scene/day/night × quality × scale; CPU środowiska, frame p95/p99, draw calls i GPU estimate; osobno server 2/100/500 klientów tam gdzie sprzęt dostępny. |

## Plan pomiaru kosztu

Po rozgrzewce 30 s zbierać po 60 s dla identycznych lokacji/kamery i konfiguracji. Zarejestrować CPU, GPU, driver, rozdzielczość, build release, lane i compiler. Rozdzielić koszt środowiska od całej klatki, CPU submission od GPU elapsed. Porównać `low` i `standard` oraz bazę w porównywalnej scenie. Raportować p50/p95/p99/max i ilość próbek, nie tylko FPS.

Dla serwera astronomia nie jest liczona per gracz. Sprawdzić, że okresowe EnvironmentSync i TimeProbe mieszczą się w opisanych limitach; dane katalogu nie są wysyłane cyklicznie. Nie deklarować spełnienia celu 500 graczy / 500 MHz bez właściwego sprzętu i istniejącej metodologii projektu. Brak tej historycznej platformy zapisać osobno; nie zastępuje to wymaganej funkcjonalnej weryfikacji E004.

## Kryteria zamknięcia

- [ ] Użytkownik wyraźnie rozpoczął końcową fazę; odniesienie do tego polecenia zapisano w ledgerze.
- [ ] V01–V20 mają wynik, dokładne wejścia, SHA, komendy i trwałe artefakty; przypadki nieadekwatne mają merytoryczne uzasadnienie.
- [ ] Wszystkie obowiązkowe scenariusze funkcjonalne i progi astronomii/synchronizacji przeszły, a znalezione defekty naprawiono i zweryfikowano w ich zakresie.
- [ ] Wizualne QA jest wykonane na wyrenderowanym kliencie, nie zastąpione samymi wartościami liczbowymi.
- [ ] Raport nie myli braku dostępu/NOT RUN z PASS; nieudowodnione cele sprzętowe pozostają jawnie niepotwierdzone.
- [ ] Po ostatniej poprawce zapisano finalny zintegrowany SHA oraz adekwatnie powtórzono tylko kontrole dotkniętych ryzyk.
- [ ] Wyniki i poprawki mają PR-y scalone do gałęzi epica; issue odsyła do dowodów.

Nie scalać końcowego PR do main w tym zadaniu; końcowy przegląd i integracja należą do E004-17.

## Docelowy PR

Osobny branch od aktualnej gałęzi `epic/004-realistic-sky-time`; PR kierować do `epic/004-realistic-sky-time`. W opisie PR powiązać kryteria odbioru z rzeczywistymi zmianami i dowodami; nie scalać tego przyrostu bezpośrednio do `main`.

