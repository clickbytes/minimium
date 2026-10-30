# Epic 004 — Realistyczny czas i niebo

**Rodzic:** [#51](https://github.com/clickbytes/minimium/issues/51). **Gałąź:** `epic/004-realistic-sky-time`.
Scalony PR integracyjny do main to [#70](https://github.com/clickbytes/minimium/pull/70). Data przygotowania planu: 2026-10-30.

## Cel i rezultat dla gracza

Dodać do Minimium wspólny czas świata, płynny dzień/noc, podstawowe niebo, Słońce, Księżyc z rzeczywistą fazą, 500 rzeczywistych gwiazd oraz zgodne z nimi oświetlenie i lokalne cienie. Dla ustalonej daty UTC i geograficznej pozycji gracza na Earth układ nieba ma odpowiadać rzeczywistemu niebu w zadanym budżecie błędu.

**Stan: ZAMKNIĘTY NA POLECENIE UŻYTKOWNIKA.** Kod scalono do `main` w [PR #70](https://github.com/clickbytes/minimium/pull/70), merge `33efd93ef3691b7e73f13531db4f549fdc1ebc0f`. E004-15 ma STATIC PASS. E004-16 zamknięto bez wykonania V01–V20; runtime, GPU i QA wizualne pozostają NOT RUN. [Rejestr zamknięcia](epic-004-closeout.md) rozróżnia dostarczenie kodu od weryfikacji.

Baza planu: [`main@aad2c82`](https://github.com/clickbytes/minimium/commit/aad2c8298606a46dc3a286b4eac7f1de45759851). Przed implementacją gałąź integracyjna została porównana i zsynchronizowana z [`main@30ff278`](https://github.com/clickbytes/minimium/commit/30ff278f9a0020f93d2567b2ca5c05a290bc605e) przez `cd816c2`. Zachowuje to nowszą konsolę, suwak zoomu i kafelkowany LOD globusa. Historyczny baseline miał protokół v11, save v7 i statyczne światło. Obecna integracja ma v13/v9 oraz implementację zegara i nieba; jej runtime pozostaje NOT RUN.

## Zakres obowiązkowy

| Obszar | Wymagany rezultat |
| --- | --- |
| Czas | `real_time` = UTC serwera; `time_scale` = wybrana data i tempo 0.1..1 000 000× (lub 0× dla pauzy). Pauza środowiska przez 0×. Fizyka, cooldowny i deadline nadal używają realnego czasu. |
| Autorytet i trwałość | Jeden ClockId dla planety i dungeon; admin commands; zapis v9; migracje v2–v7; jawne pause/advance podczas offline; brak utraty istniejących danych. |
| Multiplayer | Protokół v13, stan w join/prepare/commit, okresowa i zdarzeniowa synchronizacja, pomiar t1–t4, kompletny lifecycle portali/reconnectu. |
| Obserwator | Szerokość/długość z Position3Mm, jawna fizyczna wysokość i jedna konwersja osi; Earth 650 km pozostaje skalą geometrii, nie modelem fizycznej Ziemi dla paralaksy. |
| Astronomia | Przypięty Astronomy Engine C za safe Rust API; obliczenia f64 offline; Słońce, Księżyc, precesja/obrót gwiazd i ograniczony cache według czasu symulowanego. |
| Gwiazdy | Dokładnie 500 globalnych rekordów HYG 4.1 wybranych według apparent V magnitude i HYG id. Odtwarzalny importer, prawdziwy hash, ruch własny, kolory, jeden batch GPU. |
| Księżyc | Rzeczywista pozycja, odległość i rozmiar kątowy; ciągła faza oraz właściwy kierunek jasnej części z geometrii; widoczność także w dzień i zasłanianie gwiazd ciemną tarczą. |
| Dzień/noc | Wysokość Słońca steruje niebem, zmierzchem i widocznością gwiazd; obsłużone obie półkule i polarny dzień/noc. |
| Oświetlenie | Jeden stan astronomiczny dla terenu, wody i globusa; globalny kierunek Słońca oddzielony od lokalnego wygaszenia przy obserwatorze; poprawne normalne. |
| Cienie | Jedna lokalna mapa od dominującego Słońca/Księżyca, jawne ograniczenie zasięgu i kompletności casterów; bez nieograniczonego doczytywania planety. |
| Jakość i odbiór | `low` i `standard` zachowują tę samą astronomię; komendy diagnostyczne, rzeczywiste metryki, statyczny audyt i osobny końcowy odbiór funkcjonalny/wizualny. |

## Główne decyzje wykonawcze

- Daty: `[2000-01-01T00:00:00Z,2101-01-01T00:00:00Z)`, POSIX UTC ms w `i64`. Skala w ppm `u64`, 1× = 1 000 000, obliczenia checked `i128`. Absolutna data nie trafia do GPU jako `f32`.
- Jeden zegar `ClockId=1`; `ClockRevision` i 16-bajtowy nonce uruchomienia są odrębne od rewizji modelu, hasha katalogu oraz world content revision. Zmiana daty nie zmienia seedów ani geometrii.
- Profile `Interior/Earth/EarthLike` są osobnym opisem środowiska. Dungeon zachowuje czas, choć nie rysuje ziemskiego nieba. Proceduralna planeta używa jawnie nazwanego mapowania ziemskiego nieba.
- Astronomy Engine `v2.1.19` przypinamy do commita `61dc07020aaa6885d2c7f688a4d82beaf6edb9ef`. C/FFI występuje wyłącznie w ścieżce klienta; server/core/protocol nie zyskują tej zależności.
- HYG 4.1 przypinamy do commita `c7f7f883fe678cc7680169a50ccd7dcc49b060ce`. Katalog ma id 1 i rzeczywisty SHA-256 wygenerowanego assetu; nie publikujemy wymyślonych hashy. Licencja danych CC BY-SA 4.0 jest opisana osobno od licencji kodu.
- `standard`: mapa cieni 1024×1024, lokalny odbiór 128 m; `low`: mapa wyłączona. Jakość nie wybiera innych gwiazd ani innej fazy.
- Cel dokładności backendu dla zgodnych UTC/observer/frame: Sun/Moon >5° nad horyzontem ≤0,05°, udział oświetlonej tarczy ±0,002, sama interpolacja ≤0,001°. To kryteria późniejszego odbioru, nie wyniki pomiaru.
- Parametry i kompletne algorytmy są opisane w kartach wykonawczych. Zmiana wspólnego kontraktu wymaga aktualizacji dotkniętych kart i dokumentu kontraktów w jednym przyroście.

## Ograniczenia modelu

Nie odwzorowujemy pełnej atmosfery, bieżącej pogody, chmur, planet Układu Słonecznego, zaćmień, dokładnych globalnych cieni terenu, CSM, etykiet konstelacji ani dat poza przyjętym zakresem. Gwiazdy V1 pomijają refrakcję atmosferyczną oraz roczną paralaksę/aberrację; Sun/Moon mają osobne geometryczne i refraktowane kierunki.

Pomniejszony glob i relief zmieniają widoczny horyzont; pozycje astronomiczne i wizualne zasłanianie mają osobno opisane znaczenie. Nie obiecujemy zgodności rzeczywistego momentu wschodu co do sekundy wobec pogody i lokalnych gór. Cel historycznego sprzętu pozostaje do pomiaru, bez deklaracji wydajności na podstawie samego planu.

## Zadania wykonawcze

Każde poniższe issue ma własny cel, konkretne istniejące i nowe pliki, kontrakty, kolejność zmian, walidację i błędy, przypadki brzegowe, kryteria odbioru, wyłączenia zakresu oraz sposób dostarczenia PR. Zależności odnoszą się do zakończonej integracji blokera w branchu epica.

- [x] #52 — **E004-00**: Zamrożenie kontraktów czasu i nieba, ADR oraz baseline integracji
- [x] #53 — **E004-01**: Wprowadzić czyste typy czasu świata i kontrakt środowiska w minimium-core
- [x] #54 — **E004-02**: Uruchomić autorytatywny zegar serwera i parametryczne polecenia administracyjne czasu
- [x] #55 — **E004-03**: Zapisywać zegar świata w save v8 i zachować dane podczas migracji oraz restartu
- [x] #59 — **E004-04**: Rozszerzyć protokół do v12 o stan środowiska i synchronizację czasu we wszystkich aplikacjach
- [x] #60 — **E004-05**: Dodać klientowy estymator czasu, wygładzanie i poprawny lifecycle portali, reconnect oraz demo
- [x] #56 — **E004-06**: Georeferencja obserwatora: Earth 650 km, ramka N/E/U i jawny model nieba
- [x] #57 — **E004-07**: Astronomia offline: przypięty Astronomy Engine C, bezpieczny Rust FFI i wspólny SkyState
- [x] #58 — **E004-08**: Deterministyczny katalog 500 gwiazd HYG: importer, asset, tożsamość i licencja
- [x] #61 — **E004-09**: Wspólny EnvironmentFrame, kontrakt GPU i poprawne normalne geometrii
- [x] #62 — **E004-10**: Niebo, świt i zmierzch, Słońce oraz spójne oświetlenie sceny i globusa
- [x] #63 — **E004-11**: Katalog 500 gwiazd w jednym batchu, prawdziwa orientacja i jasność
- [x] #64 — **E004-12**: Tarcza Księżyca z rzeczywistą fazą i orientacją oraz światło księżycowe
- [x] #65 — **E004-13**: Budżetowane lokalne cienie Słońca lub Księżyca z bezpiecznym fallbackiem
- [x] #66 — **E004-14**: Profile jakości, komendy diagnostyczne i mierniki kosztu środowiska
- [x] #67 — **E004-15**: Audyt integracji czasu–astronomii–renderera i aktualizacja dokumentacji
- [ ] #68 — **E004-16**: Końcowa weryfikacja astronomii, synchronizacji, migracji i grafiki
- [ ] #69 — **E004-17**: Końcowy przegląd, integracja epica do main i zamknięcie rejestru

## Zależności i rejestr wykonania

| Zadanie | Blokery przed rozpoczęciem | Status implementacji |
| --- | --- | --- |
| [E004-00 / #52](https://github.com/clickbytes/minimium/issues/52) | brak | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#71](https://github.com/clickbytes/minimium/pull/71), merge `12f84a7`) |
| [E004-01 / #53](https://github.com/clickbytes/minimium/issues/53) | [E004-00 / #52](https://github.com/clickbytes/minimium/issues/52) | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#73](https://github.com/clickbytes/minimium/pull/73), commit `a8577b4`) |
| [E004-02 / #54](https://github.com/clickbytes/minimium/issues/54) | [E004-01 / #53](https://github.com/clickbytes/minimium/issues/53) | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#74](https://github.com/clickbytes/minimium/pull/74), code commit `c9a6855`) |
| [E004-03 / #55](https://github.com/clickbytes/minimium/issues/55) | [E004-01 / #53](https://github.com/clickbytes/minimium/issues/53), [E004-02 / #54](https://github.com/clickbytes/minimium/issues/54) | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#75](https://github.com/clickbytes/minimium/pull/75), merge `77218a6`) |
| [E004-04 / #59](https://github.com/clickbytes/minimium/issues/59) | [E004-01 / #53](https://github.com/clickbytes/minimium/issues/53), [E004-02 / #54](https://github.com/clickbytes/minimium/issues/54), [E004-03 / #55](https://github.com/clickbytes/minimium/issues/55), [E004-08 / #58](https://github.com/clickbytes/minimium/issues/58) | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#79](https://github.com/clickbytes/minimium/pull/79), commit `7c49d92`) |
| [E004-05 / #60](https://github.com/clickbytes/minimium/issues/60) | [E004-01 / #53](https://github.com/clickbytes/minimium/issues/53), [E004-04 / #59](https://github.com/clickbytes/minimium/issues/59) | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#80](https://github.com/clickbytes/minimium/pull/80), commit `2d58ac2`, merge `c40db94`) |
| [E004-06 / #56](https://github.com/clickbytes/minimium/issues/56) | [E004-01 / #53](https://github.com/clickbytes/minimium/issues/53) | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#76](https://github.com/clickbytes/minimium/pull/76), code commit `15ac4d2`, merge `dfbc795`) |
| [E004-07 / #57](https://github.com/clickbytes/minimium/issues/57) | [E004-06 / #56](https://github.com/clickbytes/minimium/issues/56) | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#77](https://github.com/clickbytes/minimium/pull/77), commit `5da228e`, merge `69a633e`) |
| [E004-08 / #58](https://github.com/clickbytes/minimium/issues/58) | [E004-07 / #57](https://github.com/clickbytes/minimium/issues/57) | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#78](https://github.com/clickbytes/minimium/pull/78), commit `38157c0`; merge `cb3d70a`) |
| [E004-09 / #61](https://github.com/clickbytes/minimium/issues/61) | [E004-05 / #60](https://github.com/clickbytes/minimium/issues/60), [E004-06 / #56](https://github.com/clickbytes/minimium/issues/56), [E004-07 / #57](https://github.com/clickbytes/minimium/issues/57) | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#81](https://github.com/clickbytes/minimium/pull/81), commit `6c19d2e`, merge `3155a2c`) |
| [E004-10 / #62](https://github.com/clickbytes/minimium/issues/62) | [E004-09 / #61](https://github.com/clickbytes/minimium/issues/61) | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#82](https://github.com/clickbytes/minimium/pull/82), merge `45b4f29`) |
| [E004-11 / #63](https://github.com/clickbytes/minimium/issues/63) | [E004-08 / #58](https://github.com/clickbytes/minimium/issues/58), [E004-09 / #61](https://github.com/clickbytes/minimium/issues/61), [E004-10 / #62](https://github.com/clickbytes/minimium/issues/62) | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#83](https://github.com/clickbytes/minimium/pull/83), merge `325ab46`) |
| [E004-12 / #64](https://github.com/clickbytes/minimium/issues/64) | [E004-09 / #61](https://github.com/clickbytes/minimium/issues/61), [E004-10 / #62](https://github.com/clickbytes/minimium/issues/62) | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#84](https://github.com/clickbytes/minimium/pull/84), merge `4750485`) |
| [E004-13 / #65](https://github.com/clickbytes/minimium/issues/65) | [E004-12 / #64](https://github.com/clickbytes/minimium/issues/64) | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#85](https://github.com/clickbytes/minimium/pull/85), merge `a27e79f`) |
| [E004-14 / #66](https://github.com/clickbytes/minimium/issues/66) | [E004-03 / #55](https://github.com/clickbytes/minimium/issues/55), [E004-05 / #60](https://github.com/clickbytes/minimium/issues/60), [E004-11 / #63](https://github.com/clickbytes/minimium/issues/63), [E004-12 / #64](https://github.com/clickbytes/minimium/issues/64), [E004-13 / #65](https://github.com/clickbytes/minimium/issues/65) | IMPLEMENTED / FINAL VERIFICATION PENDING (PR [#86](https://github.com/clickbytes/minimium/pull/86), merge `93b1264502e33f01a9769901fbbc72697bf904e0`) |
| [E004-15 / #67](https://github.com/clickbytes/minimium/issues/67) | [E004-01 / #53](https://github.com/clickbytes/minimium/issues/53), [E004-02 / #54](https://github.com/clickbytes/minimium/issues/54), [E004-03 / #55](https://github.com/clickbytes/minimium/issues/55), [E004-04 / #59](https://github.com/clickbytes/minimium/issues/59), [E004-05 / #60](https://github.com/clickbytes/minimium/issues/60), [E004-06 / #56](https://github.com/clickbytes/minimium/issues/56), [E004-07 / #57](https://github.com/clickbytes/minimium/issues/57), [E004-08 / #58](https://github.com/clickbytes/minimium/issues/58), [E004-09 / #61](https://github.com/clickbytes/minimium/issues/61), [E004-10 / #62](https://github.com/clickbytes/minimium/issues/62), [E004-11 / #63](https://github.com/clickbytes/minimium/issues/63), [E004-12 / #64](https://github.com/clickbytes/minimium/issues/64), [E004-13 / #65](https://github.com/clickbytes/minimium/issues/65), [E004-14 / #66](https://github.com/clickbytes/minimium/issues/66) | STATIC PASS / FINAL VERIFICATION PENDING (accepted SHA 8a0a33d) |
| [E004-16 / #68](https://github.com/clickbytes/minimium/issues/68) | [E004-15 / #67](https://github.com/clickbytes/minimium/issues/67) | CLOSED AS NOT PLANNED FOR THIS CLOSEOUT; V01–V20 NOT RUN |
| [E004-17 / #69](https://github.com/clickbytes/minimium/issues/69) | [E004-16 / #68](https://github.com/clickbytes/minimium/issues/68) | MERGED TO MAIN IN PR #70; VERIFICATION WAIVED FOR CLOSEOUT |

Kolejność numeryczna nie zastępuje zależności: protokół E004-04 wymaga rzeczywistego hasha katalogu E004-08.
Gałęzie 02–03 i 06–08 można rozwijać po wspólnym core; oba przyrosty muszą być zintegrowane przed 04.
E004-11 i E004-12 mają wspólny punkt wejścia po 10, a 13 wymaga księżycowego źródła światła z 12.

## Zasady realizacji i statusów

Branch integracyjny: `epic/004-realistic-sky-time`. Każdy task ma osobny branch od aktualnej integracji i własny PR do tej gałęzi. PR #70 scalono do `main`; szczegóły i rzeczywisty merge SHA są w [rejestrze zamknięcia](epic-004-closeout.md).

`PLANNED` oznacza opisane zadanie; `IMPLEMENTED / FINAL VERIFICATION PENDING` oznacza scalony task i rzeczywiste dowody statyczne; `VERIFIED` wymaga końcowych wyników. Zamknięcie zadania implementacyjnego nie jest dowodem przetestowania funkcji.

[AGENTS.md](https://github.com/clickbytes/minimium/blob/aad2c8298606a46dc3a286b4eac7f1de45759851/AGENTS.md) odracza tworzenie/uruchamianie testów do wyraźnie rozpoczętej przez użytkownika końcowej fazy. Zgodnie z konwencją planów repo próby aplikacji i obciążenia również są zapisane w końcowym odbiorze. Podczas implementacji można wykonywać kompilację, formatowanie i kontrole statyczne. Własny kod/shadery pozostają ≤500 fizycznych linii na plik; wyjątki vendor/generated są dokładnie opisane.

## Definition of Done epica

- [ ] Wszystkie E004-00..15 mają własny przegląd, PR, commit integracji i rzeczywiste dowody w swoim zakresie.
- [ ] Czas działa spójnie w obu trybach, po restarcie, migracji, portalu i reconnect; nie skaluje fizyki.
- [ ] Dokładnie 500 gwiazd i rzeczywiste Sun/Moon/fazy są zgodne z pozycją oraz UTC w opisanym budżecie błędu.
- [ ] Niebo, terrain/water/globe, tarcza Księżyca i lokalne cienie używają tego samego stanu oraz właściwych osi.
- [ ] Licencje, źródła, hashe, błędy/fallbacki, limity i komendy są udokumentowane i odpowiadają kodowi.
- [ ] E004-16 ma jawnie rozpoczętą fazę końcową, zakończone obowiązkowe scenariusze i trwałe dowody; brak wyniku nie jest oznaczony PASS.
- [ ] E004-17 potwierdza końcowy review, merge do main i rzeczywisty merge SHA.
- [ ] Rodzic zamknięty dopiero po spełnieniu powyższych warunków; nie przez automatyczne zamknięcie przy scaleniu samej dokumentacji.

## Dokumenty

- [Wspólne kontrakty](epic-004-contracts.md).
- [Plan i rejestr weryfikacji](epic-004-verification.md).
- [Audyt integracji E004-15](epic-004-audit.md).
- ADR `docs/adr/0006-world-time-and-sky.md` zatwierdza kontrakty E004. Jest decyzją architektoniczną, nie dowodem implementacji funkcji.

## Stan przygotowania

E004-00 / #52 zostało scalone przez PR [#71](https://github.com/clickbytes/minimium/pull/71)
do gałęzi integracyjnej (`12f84a7`). E004-01 dostarcza czysty kontrakt core przez PR
[#73](https://github.com/clickbytes/minimium/pull/73). E004-02 jest w PR
[#74](https://github.com/clickbytes/minimium/pull/74), z osobnym adapterem serwerowym,
komendami czasu, konfiguracją nowego świata i profilami instancji. Formatowanie, kompilacja,
Clippy i kontrole statyczne przeszły; cargo zgłasza wyłącznie ostrzeżenia Windows o fallbacku
hard-link cache. Nie uruchamiano testów ani aplikacji. E004-03 scalono przez PR #75
(`77218a6`) i E004-06 (`dfbc795`). E004-07 jest gotowe w PR #77
(`5da228e`, merge `69a633e`). E004-08 merged in PR #78 (`38157c0`, merge `cb3d70a`); E004-04
was merged in PR #79 (`7c49d92`, merge `a69ff33`). E004-05 was merged in PR #80
(`2d58ac2`, merge `c40db94`); its static checks pass, while runtime verification remains deferred to E004-16.
E004-09 merged in PR #81 (`6c19d2e`, merge `3155a2c`); runtime and visual verification remain deferred to E004-16.
E004-10 was merged in PR #82 (`951b8b2`, merge `45b4f29`), E004-11 in PR #83 (`eee83e8`, merge `325ab46`), E004-12 in PR #84 (`0d162e1`, merge `4750485`), and E004-13 in PR #85 (merge `a27e79f`). E004-14 merged in PR #86 (`93b1264`). E004-15 completed the source audit and static checks, fixing integration defects; see [audit](epic-004-audit.md). The user directed closeout without E004-16 verification. Runtime for the epic is NOT RUN; see [closeout](epic-004-closeout.md).
