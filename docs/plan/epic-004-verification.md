# Epic 004 — plan i rejestr weryfikacji

**Epic:** [#51](https://github.com/clickbytes/minimium/issues/51). **Właściciel finalnego odbioru:** [E004-16 / #68](https://github.com/clickbytes/minimium/issues/68).
**Baza planu:** `aad2c8298606a46dc3a286b4eac7f1de45759851`. **Kodowy baseline E004-00:** `cd816c2d32bd4b1447f14ac04a9ec99e44c0e4af` (merge `main@30ff278` into integration). **Status:** kontrakty zatwierdzone; E004-01 zintegrowane w PR #73; E004-02 w PR #74 (`c9a6855`); E004-03 zintegrowane w PR #75 (`c7d6000`, merge `77218a6`); E004-06 zintegrowane w PR #76 (`15ac4d2`, merge `dfbc795`); E004-07 zintegrowane w PR #77 (`61956b6`, merge `69a633e`); E004-08 w PR #78 (`38157c0`, merge pending); pozostałe zadania NOT STARTED; runtime NOT RUN.

## Rejestr początkowy

| Obszar | Stan | Dowód / następny krok |
| --- | --- | --- |
| Kompletność 18 opisów i graf zależności | Przegląd planu wykonany | Cele, pliki, kroki, kontrakty, kryteria, zależności i zakres; graf acykliczny. To nie jest kontrola kodu E004. |
| Referencje do kodu | Sprawdzone | Issues zawierają historyczne ścieżki bazowe do `aad2c82`; aktualny punkt integracji `cd816c2` zachowuje zmiany main `30ff278`, w tym nowsze pliki konsoli i globusa. |
| Rust/C toolchain, target, ostrzeżenia bazowe | CAPTURED | Windows, target `x86_64-pc-windows-msvc`; `rustc 1.99.0 (b940084d7 2026-09-28)`, `cargo 1.99.0 (5f94df478 2026-08-27)`, toolchain z `rust-toolchain.toml`. Clang i GCC dostępne, MSVC `cl.exe` niewykryty; zgodność C/FFI z targetem pozostaje do ustalenia w E004-07. Cargo incremental hard-link warnings na `D:\Git\minimium\target`, fallback kopiowania. `cargo check` 0 errors / 12 warnings; Clippy 0 errors / 11 warnings. |
| Implementacja funkcji | E004-01 w PR #73; pozostałe PLANNED | Zintegrowany kontrakt core obejmuje typy i czyste funkcje; runtime nie był uruchamiany. |
| Kompilacja, fmt, clippy, architektura, limity plików | BASELINE PASS | Head `cd816c2`; `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors, 12 warnings), `cargo clippy --workspace --locked` (0 errors, 11 warnings), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (163 pliki, max 500 linii). Nie używano `-D warnings`. To baseline gałęzi epica, nie odbiór późniejszej implementacji. |
| E004-01 statyczne kontrole | PASS na `a8577b4` | `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors; 12 warnings o hard-link cache), `cargo clippy --workspace --locked` (0 errors; 12 ostrzeżeń o hard-link cache, bez lint diagnostics), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (167 plików, max 500 linii), `git diff --check`. Żadne testy ani aplikacja nie były uruchamiane. |
| E004-02 statyczne kontrole | PASS na `c9a6855` | `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors; 11 ostrzeżeń o Windows incremental hard-link fallback), `cargo clippy --workspace --locked` (0 errors; 11 tych samych ostrzeżeń, bez diagnostyk lintów), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (174 pliki, max 500 linii), `git diff --check`. Żadne testy ani aplikacja nie były uruchamiane. |
| E004-03 statyczne kontrole | PASS na `c7d6000` | PR [#75](https://github.com/clickbytes/minimium/pull/75), merge `77218a6`; `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors; 11 ostrzeżeń o Windows incremental hard-link fallback), `cargo clippy --workspace --locked` (0 errors; 11 tych samych ostrzeżeń, bez diagnostyk lintów), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (176 plików, max 500 linii), `git diff --check`. Żadne testy ani aplikacja nie były uruchamiane. |
| E004-06 statyczne kontrole | PASS na `15ac4d2` | PR [#76](https://github.com/clickbytes/minimium/pull/76), branch `feature/e004-06-observer-frame`; `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors; 12 ostrzeżeń o Windows incremental hard-link fallback), `cargo clippy --workspace --locked` (0 errors; 12 tych samych ostrzeżeń, bez diagnostyk lintów), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (181 plików, max 500 linii), `git diff --check`. Żadne testy ani aplikacja nie były uruchamiane. |
| E004-07 statyczne kontrole | PASS na `5da228e` | PR [#77](https://github.com/clickbytes/minimium/pull/77), branch `feature/e004-07-astronomy`; `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors; 13 ostrzeżeń o Windows incremental hard-link fallback), `cargo check --workspace --release --locked`, `cargo build --workspace --release --locked` (statyczne linkowanie Astronomy Engine C na `x86_64-pc-windows-msvc`), `cargo clippy --workspace --locked -- -D warnings` (pass; ostrzeżenia tylko o incremental hard-link fallback), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (196 plików, max 500 linii), `git diff --check`. Brak testów, porównań astronomicznych i uruchomienia aplikacji; final verification pozostaje odroczone do E004-16. |
| E004-08 statyczne kontrole | PASS na `38157c0` | PR [#78](https://github.com/clickbytes/minimium/pull/78), branch `feature/e004-08-star-catalogue`; `cargo fmt --all -- --check`, `cargo check --workspace --locked` (0 errors; 14 ostrzeżeń o Windows incremental hard-link fallback), `cargo clippy --workspace --locked -- -D warnings` (0 lint errors; te same ostrzeżenia), `cargo build --workspace --release --locked` z tymczasowym `CARGO_TARGET_DIR` (pass; standardowy katalog release miał zablokowane pliki client/server), `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py` (204 pliki, max 500 linii), `ruff check`, `python -m compileall`, `git diff --check`. Import pinned HYG: CSV 33 932 548 B, Git blob `ba2dec4e…ddf84`, SHA-256 `d9f69fd8…e4ebd`; 119 626 wierszy, 500 rekordów, asset 32 096 B, SHA-256 `81c8d6e0…e3db`. Żadne testy ani aplikacja nie były uruchamiane; final verification pozostaje odroczone do E004-16. |
| Unit/integration/e2e, aplikacja i loadtest | NOT RUN | Wyraźne rozpoczęcie końcowej fazy przez użytkownika oraz ukończone E004-15. |
| Wizualne QA i astronomiczne referencje | NOT RUN | Nie zastępować wartościami z planu ani wcześniejszymi wynikami E002/E003. |
| Sprzęt historyczny / 500 graczy | NOT MEASURED | Raportować dostępność platformy i realne pomiary; nie deklarować na podstawie planu. |

## Wypełniany ledger scenariuszy

| Scenariusz | Wynik | Head SHA | Środowisko / wejścia | Dowód / defekt |
| --- | --- | --- | --- | --- |
| V01 | NOT RUN | — | — | — |
| V02 | NOT RUN | — | — | — |
| V03 | NOT RUN | — | — | — |
| V04 | NOT RUN | — | — | — |
| V05 | NOT RUN | — | — | — |
| V06 | NOT RUN | — | — | — |
| V07 | NOT RUN | — | — | — |
| V08 | NOT RUN | — | — | — |
| V09 | NOT RUN | — | — | — |
| V10 | NOT RUN | — | — | — |
| V11 | NOT RUN | — | — | — |
| V12 | NOT RUN | — | — | — |
| V13 | NOT RUN | — | — | — |
| V14 | NOT RUN | — | — | — |
| V15 | NOT RUN | — | — | — |
| V16 | NOT RUN | — | — | — |
| V17 | NOT RUN | — | — | — |
| V18 | NOT RUN | — | — | — |
| V19 | NOT RUN | — | — | — |
| V20 | NOT RUN | — | — | — |

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
| V01 | CoreClock: 0/1/60/240/1440x, zmiany skali, set date, RealTime vs TimeScale1, granice dat, overflow i niepoprawne ppm; brak zależności od liczby ticków. |
| V02 | Server: pominięte ticki i długie przerwy, brak graczy, korekty SystemTime w obie strony; fizyka/cooldown/timeout/autosave nie otrzymują skali. |
| V03 | Wire: round-trip wszystkich nowych ramek, maksymalne długości i ucięcie, niewłaściwe tagi/nonce/id/revision/context, wersja11↔12, rate limit; v12 obsługuje wszystkie aplikacje. |
| V04 | Dwa klienty z innym zegarem systemowym: ta sama próbka świata i niezależna lokalizacja; 4-probe bootstrap, pojedyncze inflight i okres 5 s, brak nieograniczonego ruchu. |
| V05 | Kontrolowane symetryczne RTT 0/50/200ms i jitter; po stabilizacji (8 ważnych próbek) oszacowanie monotonicznego offsetu w granicy max(10ms, połowa mierzonego jitteru). Test asymetrii raportuje niepewność, nie udaje gwarancji niemożliwej bez zewnętrznej synchronizacji. |
| V06 | Reconnect i restart server nonce, spóźnione odpowiedzi, stary context/revision; nie wraca stara data ani opis świata. |
| V07 | Planet→dungeon→planet: prepare/cancel/ready/commit, brak przełączania sky przed commit, czas biegnie w Interior; teleport i zmiana render_origin nie odbijają osi. |
| V08 | Save8: manual/autosave/final save, pause/advance offline, clock backward, endpointy zakresu; opóźnienie workera nie zmienia daty snapshotu. |
| V09 | Migracja wszystkich aktualnie obsługiwanych save v2–v7 z zachowaniem ich własnych reguł fingerprint/manifest; dane niezgodne nadal odrzucone, brak resetu danych przy błędzie. |
| V10 | Atomowość: błąd zapisu, za duży snapshot, nieznane pola/duplikaty zegara; poprzedni poprawny zapis pozostaje dostępny. |
| V11 | Astronomia: miejsca i daty z referencji, obu półkul, precesja/ruch własny, granica±180°, bieguny/zenit; geometryczna i pozorna pozycja rozróżnione. |
| V12 | Interpolacja: porównanie direct backend vs cache przy skali0..1440 i skoku lat; błąd interpolacji kierunku <=0.001° względem bezpośredniego backendu, lokalne obroty co klatkę, brak odtwarzania pominiętych przedziałów. |
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

