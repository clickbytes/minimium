# E004-16 — procedura końcowego odbioru

**Status: NOT STARTED / RUNTIME NOT RUN.** Ten dokument przygotowano w E004-15.
Wykonanie wymaga zakończenia #67 oraz osobnego polecenia użytkownika rozpoczynającego
fazę testową. Akceptacja planu E004-15 nie jest taką zgodą. Ledger wyników pozostaje
w [epic-004-verification.md](epic-004-verification.md); poniżej są instrukcje, nie wyniki.

## Organizacja i dowody

1. A1 zapisuje treść zgody i datę, actual merge SHA #67 oraz czystość checkoutu.
   Tworzy `feature/e004-16-verification`; A2–A4 zaczynają od tego samego SHA w osobnych
   worktree z osobnymi `CARGO_TARGET_DIR`, katalogami danych/profili i portami loopback.
2. Nadać unikalny `run_id`. Ścieżka: `docs/plan/evidence/epic-004/<run_id>/<agent>/`.
   Duże capture przechowywać trwale poza Git, z URL, rozmiarem i SHA-256 w manifeście.
   Nie zatrzymywać procesów użytkownika; harness sprząta tylko własne PID-y.
   Przed rekurencyjnym usunięciem sprawdzić bezwzględną ścieżkę w katalogu run_id.
3. Każdy podprzypadek zapisuje V-ID, SHA/dirty, pełną komendę i exit code, UTC,
   OS/CPU/GPU/driver/backend, toolchain i build; seed/preset, pozycję, kamerę,
   datę świata, skalę, requested/effective quality; expected/measured/tolerance/result;
   ścieżki, rozmiary i SHA-256 artefaktów. `n/a` wymaga wyjaśnienia.
4. A1 prowadzi defekty: objaw, reprodukcja, właściciel, poprawka, regresja,
   SHA naprawy i powtórzony wynik. Poprawka unieważnia wcześniejsze dotknięte V-ID.
   Nie zwiększać tolerancji ani nie zastępować referencji wynikiem badanego backendu.
5. Po integracji poprawek zamrozić SHA; uruchomić `rtk cargo test --workspace --locked`
   oraz `rtk cargo test -p minimium-server --features allocation-metrics --locked`.
   Testy procesowe i GPU oznaczyć ignored w zwykłym przebiegu i uruchamiać osobno
   dokładnymi poleceniami z manifestów agentów. Nie uznawać zwykłego `cargo test`
   za dowód tych ignorowanych scenariuszy. Zapisać faktyczne nazwy harnessów i filtry.
6. V20 i wizualny odbiór wspólnego GPU wykonuje tylko A4, bez kompilacji i innego
   obciążenia hosta. A2 uzgadnia osobne okno pomiaru serwera. A1 zamyka #68 dopiero
   po scaleniu PR i rozliczeniu wszystkich wymaganych przypadków.

## Własność scenariuszy

| V-ID | Właściciel | Zakres i wymagany dowód |
| --- | --- | --- |
| V01–V02 | A2 | Czysta logika czasu oraz adapter; wstrzyknięte UTC/monotonic, bez zmiany zegara hosta. |
| V03–V06 | A2 | Wire i rzeczywisty serwer loopback z kontrolowanymi klientami/proxy; desktop, terminal i loadtest v12. |
| V07 | A2 + A4 | Lifecycle przejść z zapisanym śladem sieciowym oraz potwierdzeniem obrazu. |
| V08–V10 | A2 | Snapshoty i każdy legacy v2–v7, opóźniony worker, błędy atomowego zapisu. |
| V11–V12 | A3 | Niezależne zamrożone referencje, jednostki, FFI/ABI/concurrency, cache i katalog. |
| V13–V18 | A4 | Produkcyjne pipeline'y na GPU oraz obraz uruchomionego klienta, obie jakości. |
| V19 | A2 + A4 | Admin/lifecycle + CLI/F1/help/completion; wspólny manifest. |
| V20 | A2 + A4; A1 koordynuje | Rzeczywiste pomiary serwera i klienta na zamrożonym buildzie. |

## A2 — czas, transport, zapis

- V01: 0/1/60/240/1440× i dziesiętne ppm; ciągłość zmiany skali, set, RealTime vs
  TimeScale1, granice dat/overflow. Ta sama delta monotoniczna daje wynik niezależny
  od liczby ticków. V02: pominięte ticki, długa pauza, pusty serwer, wall-clock
  korekty w obu kierunkach; sprawdzić okres 5 s i próg 250 ms z kontraktu.
- V03: nowe wiadomości round-trip, wszystkie ucięcia, maksymalne rozmiary,
  złe tagi/wersje/ID/nonce/context, limity ruchu, brak częściowej mutacji. Wymusić
  częściowy zapis TCP przy TransitionCommitted i nasycenie wejść portal/teleport.
- V04–V05: dwie sesje, różne lokalizacje i wall clocks; cztery próby bootstrap,
  jedna inflight, osiem zaakceptowanych próbek, okres 5 s. RTT symetryczne
  0/50/200 ms i deterministyczny jitter: błąd offsetu ≤max(10 ms, połowa jitteru).
  Dla asymetrii 90/10 raportować niepewność bez gwarancji dokładności.
- V06: restart nonce, reconnect, spóźnione/zdublowane odpowiedzi, stare revision
  i context nie przywracają starego świata/czasu. V07: planet→dungeon→planet,
  cancel przed i po Ready, stale Cancel (ID/context), opóźnione ready, stale commit
  i teleport; środowisko zmienia
  się z aktywnym światem, czas biegnie w Interior.
- V08–V10: manual/autosave/final, pause/advance offline, cofnięty wall clock,
  granice dat, opóźniony worker; osobny fixture każdej wersji v2–v7. Uszkodzenia,
  duplikat CLOCK, limit rozmiaru i odmowa zapisu zachowują poprzedni snapshot.
- V19: tylko istniejący admin lub konsola klienta posiadającego lokalny proces serwera może mutować czas; połączenie attached/remote oraz niepełne polecenia nie mutują.
  Owned server zatrzymywany przez właściciela, attached pozostaje uruchomiony.

## A3 — referencje, FFI i katalog

- Sun/Moon: zamrozić surowe odpowiedzi [JPL Horizons](https://ssd-api.jpl.nasa.gov/doc/horizons.html).
  Fazy/sezony 2026 wybrać z [USNO](https://aa.usno.navy.mil/data/MoonPhases).
  Gwiazdy: niezależna transformacja ERFA/Astropy zgodna z uproszczeniami V1.
  Żaden wzorzec nie może pochodzić wyłącznie z Astronomy Engine.
- Zapisać parametry zapytań, wersje, hashe, jednostki/frame, geocentric/topocentric,
  geometric/refracted, UT1/UTC/TT, ΔT/EOP i przyszłe sekundy przestępne zgodnie z
  [manualem Horizons](https://ssd.jpl.nasa.gov/horizons/manual.html). Po pobraniu offline.
- Miejsca: Bergen, Warszawa, 0°/0°, Sydney, Tromsø, −69,6492°/18,9553°;
  długości ±179,999° przy szerokości 0° i 60°, oba bieguny, obie strony każdej
  krawędzi i narożnika cube-sphere. Wysokości fizyczne 0/1000 m oraz osobny
  przypadek przeliczenia wysokości realnego gracza na pomniejszonej Earth.
- Daty: 2000-01-01, 2050-01-01, 2100-12-31 i ostatnia dozwolona ms; referencyjne
  równonoce/przesilenia 2026, rzeczywiste nowie/kwadry/pełnie i oba sierpy.
  UTC scen zapisać w fixture, nie wyznaczać przez badaną bibliotekę.
- V11: Sun/Moon >5° separacja wektorów ≤0,05°; fraction błąd ≤0,002;
  rozmiar Księżyca ≤1% (promień vs średnica); terminator ≤1° przy fraction
  0,1–0,9 z prawidłową jasną stroną; gwiazdy ≤0,1°. Biegun/horyzont/zenit bez
  NaN, skoków i błędów orientacji. Sprawdzić transpozycję C/Rust i determinant −1
  ramki world; dodatnia ortonormalna rotacja EQJ→HOR pozostaje wymagana.
- V12: cache vs direct przy skalach 0/1/60/240/1440, początek/środek/koniec
  300 s i ±1 ms na granicach; revision, teleport, powrót i skok 2000→2100.
  Błąd interpolacji ≤0,001°; najwyżej dwa nowe końce, bez odrabiania przedziałów.
- Odtworzyć katalog do katalogu tymczasowego; porównać dokładny hash, 500 ID
  i kolejność. Nie nadpisywać wzorca. Sprawdzić ABI i równoległe, niezależnie
  konstruowane kalkulatory przez wspólną blokadę. Przekazać A4 deterministyczne
  sceny: UTC/miejsce/kierunki/faza/widoczność/orientacja jasnej części.

## A4 — GPU, obraz, interfejs

- Walidować produkcyjne WGSL; osobne ignorowane testy tworzą rzeczywiste pipeline'y
  i bind group, dla sRGB oraz liniowej powierzchni z encode pass. Kontrolowany
  harness ma produkcyjne prepare/render, stałą kamerę/scenę, reset pomiarów po
  rozgrzewce i ograniczony eksport. Bez publicznych komend testowych protokołu.
- V13: dzień/noc/świt/zmierzch/polar day/night, wschód/zachód, terminator globusa.
  V14: te same 500 gwiazd w obu jakościach, jeden batch, zasłanianie przez planetę,
  teren i całą tarczę Moon, także w widoku globu. Sprawdzić rozmiary/jasności od magnitudo, niezależne
  migotanie gwiazd bez zmiany po `timescale`, brak migotania nieba/tła oraz stabilny
  koszt 500 instancji. V15: sceny A3, obie półkule, dzienny Księżyc,
  jasna strona/rozmiar i brak czarnego wycięcia na dziennym niebie.
  W globie potwierdzić geocentryczną orbitę sfery Moon przy zmianie UTC,
  promień i dystans skalowane jednakowo względem Earth, fazę z normalnych
  powierzchni oraz głębię/zasłanianie przez glob. Sprawdzić osobne ciepłe
  halo Sun i chłodniejsze halo Moon: pełnia, kwadry, sierpy, nów, obie
  orientacje jasnej strony i zachowanie przy horyzoncie.
  W V13/V15 obrócić kamerę globu: Słońce i Księżyc mają pojawiać się we
  właściwych kierunkach, a glob zasłaniać ich tarcze po przeciwnej stronie.
  Zmierzyć średnicę obu tarcz w pikselach względem promienia kątowego z
  astronomii i FOV kamery, przy kilku rozdzielczościach i zoomach. Sprawdzić
  jasność Księżyca dla pełni, kwadr i sierpów na tle przestrzeni oraz brak
  powiększonego artefaktu Słońca za kamerą.
- V16: wszystkie normalne cube/terrain, local/global, nocna woda, Interior bez
  planetarnego światła. V17: caster poza frustum, niskie Sun, Sun→Moon→none,
  bias/PCF/fade, teleport/rebase, incomplete coverage/budżet; fallback wyłącza
  cienie i podaje przyczynę.
- V18: start okna 1920×1080 na dostępnym ekranie, 50 low↔standard cykli,
  resize, minimalizacja 60 s, skipped surface,
  odtworzenie zasobów i reconnect, bez wzrostu utrzymywanych zasobów silnika.
  Wymusić skipped acquisition przy nowej epoce gwiazd: stamp publikować dopiero
  po uploadzie. Cancel przed/po Ready i disconnect/reconnect mają zwolnić zasoby
  przygotowanego celu; stale Cancel nie może zwolnić aktywnego świata.
  Błędy alokacji/loss wymuszać hookiem/adapterem bez wyczerpywania RAM hosta;
  syntetyczny fallback nie zastępuje realnego pipeline'u GPU.
- V19: CLI wartości/duplikaty, F1/help/completion/time status/time scale 0.1|1|1000/sky info/profile, odpowiedź serwera i odrzucenie attached/remote,
  UTF-8 i poprzedni model konsoli. Regresja globusa: limit tekstury 2048, duży
  zoom, kierunek scrolla, slider, zmiana LOD, wybór teleportu; bez nowego systemu map.
- Zapisać screenshots/capture, manifest scen i ocenę obrazu uruchomionego klienta.
  `sky info` i walidacja WGSL nie zastępują QA wizualnego. Przy automatyzacji UI
  odczytać skill computer-use; wyłącznie A4 steruje oknami/GPU badanego hosta.

## V20 i bramka zamknięcia

Klient: dzień/noc × low/standard × skale 0/1/60/240/1440, identyczna scena/kamera,
30 s rozgrzewki i 60 s pomiaru każdego przebiegu. Raportować p50/p95/p99/max,
liczbę próbek całej klatki, CPU przygotowania środowiska, CPU uploadu oraz
przygotowania cieni (jawny zakres); draw calls/skips, pamięć silnika i adapter.
GPU elapsed pozostaje `n/a` bez rzeczywistych timestampów GPU.

Serwer: clustered/dispersed × 2/100/500 klientów × lanes 1/2/auto, każdy 60 s
pomiaru w nowym katalogu, faktyczna liczba połączeń, błędy i delty synchronizacji.
Środowisko musi być izolowane od innych kompilacji/obciążeń.

Brak wymaganego GPU, referencji lub środowiska to BLOCKED, nie PASS. Brak
historycznego Pentium 500 MHz raportować osobno jako NOT MEASURED. #68 wymaga
wszystkich obowiązkowych wyników i scalonych poprawek. #69 dopiero potem przegląda
aktualny main, ponawia dotknięte scenariusze, scala #70 i zapisuje faktyczny merge
SHA osobnym późniejszym PR dokumentacyjnym; #69/#51 pozostają otwarte do tej bramki.
