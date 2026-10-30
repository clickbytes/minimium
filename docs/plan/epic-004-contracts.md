# Epic 004 — kontrakty czasu, astronomii i renderera

## Status i źródło prawdy

Kontrakty zatwierdzone w E004-00 i zapisane w [ADR 0006](../adr/0006-world-time-and-sky.md).
Stan bazowy kodu: `main@30ff278f9a0020f93d2567b2ca5c05a290bc605e`; gałąź integracyjna została zsynchronizowana
przez merge commit `cd816c2d32bd4b1447f14ac04a9ec99e44c0e4af` (`epic/004-realistic-sky-time`). Dokument określa
docelową implementację, nie dowodzi, że funkcje już istnieją.
Pełne instrukcje wykonawcze, błędy i kryteria są w issues z [indeksu epica](epic-004.md).
Zmiana wspólnego kontraktu wymaga aktualizacji tego dokumentu, ADR i wszystkich
dotkniętych issues w jednym przyroście.

## 1. Właściciele danych i granice zależności

| Warstwa | Odpowiedzialność | Właściciel |
| --- | --- | --- |
| `minimium-core` | Zwalidowane typy czasu, czysta ewaluacja, profile, ObserverFrame; bez I/O, zegara OS i GPU. | [E004-01 / #53](https://github.com/clickbytes/minimium/issues/53), [E004-06 / #56](https://github.com/clickbytes/minimium/issues/56) |
| Server | Jedna autorytatywna kotwica czasu, admin, zapis, próbki i descriptor instancji. | [E004-02 / #54](https://github.com/clickbytes/minimium/issues/54), [E004-03 / #55](https://github.com/clickbytes/minimium/issues/55), [E004-04 / #59](https://github.com/clickbytes/minimium/issues/59) |
| Protocol | Jawne kodowanie LE i limity; nie oblicza astronomii. | [E004-04 / #59](https://github.com/clickbytes/minimium/issues/59) |
| Client session | Estymator offsetu, wygładzony czas renderowania, current/pending context i lifecycle. | [E004-05 / #60](https://github.com/clickbytes/minimium/issues/60) |
| `minimium-astronomy` | Jednowątkowy safe wrapper C, f64, Sun/Moon, obroty i ruch własny; bez assets/procgen/platform/protocol/GPU. | [E004-07 / #57](https://github.com/clickbytes/minimium/issues/57) |
| `minimium-assets` | Katalog i lekka tożsamość id/hash; bez zależności od astronomy. | [E004-08 / #58](https://github.com/clickbytes/minimium/issues/58) |
| Renderer | Jeden EnvironmentFrame i wspólne GPU layouty, prezentacja, oświetlenie/cienie. | [E004-09 / #61](https://github.com/clickbytes/minimium/issues/61), [E004-10 / #62](https://github.com/clickbytes/minimium/issues/62), [E004-11 / #63](https://github.com/clickbytes/minimium/issues/63), [E004-12 / #64](https://github.com/clickbytes/minimium/issues/64), [E004-13 / #65](https://github.com/clickbytes/minimium/issues/65) |
| Diagnostyka i odbiór | Jakość/metria, dokumentacja integracji, końcowe dowody i merge. | [E004-14 / #66](https://github.com/clickbytes/minimium/issues/66), [E004-15 / #67](https://github.com/clickbytes/minimium/issues/67), [E004-16 / #68](https://github.com/clickbytes/minimium/issues/68), [E004-17 / #69](https://github.com/clickbytes/minimium/issues/69) |

Serwer nie linkuje backendu astronomicznego C nawet przechodnio. Każdy klient liczy niebo dla własnej pozycji.
Nie tworzymy 500 encji, świateł ani okresowych wiadomości z pozycjami gwiazd.

## 2. Domena czasu

| Element | Zamrożony kontrakt V1 |
| --- | --- |
| UTC | `WorldTimeMs(i64)`, POSIX ms, `946684800000 <= t < 4133980800000`. |
| Daty | Od 2000-01-01T00:00:00Z do końca 2100; 2101-01-01 wyłączone. |
| Skala | `TimeScalePpm(u32)`, 0..=1 440 000 000; 1×=1 000 000. |
| ClockId | Niezerowy u64; obecnie jedynie 1 dla wszystkich instancji. |
| Revision | Niezerowy u64; rośnie przy przyjętej mutacji/korekcie, nie przy odczycie lub heartbeat. |
| Tryb | `RealTime=0`, `TimeScale=1`. |
| Offline | `Pause=0`, `Advance=1`; domyślnie Pause. |
| Sesja | 16-bajtowy losowy nonce per uruchomienie serwera; nie jest zapisywany do save. |
| Monotonic ms | u64 od początku konkretnego procesu/sesji, ważny tylko z tym samym nonce. |
| Profile | `Interior=0`, `Earth=1`, `EarthLike=2`; zawsze ClockId 1. |
| Model | `astronomy_model_revision:u32=1`. |
| Katalog | `catalogue_id:u32=1`, `catalogue_sha256:[u8;32]` całego wygenerowanego assetu. |

Ewaluacja od kotwicy: `T(now)=T(anchor)+floor((now-anchor)*rate_ppm/1_000_000)`.
Checked i128 dla różnic, mnożenia i dodawania; dopiero potem sprawdzenie zakresu i konwersja.
Nie akumulować dt/ticków i nie przesyłać daty do GPU jako f32.

`real_time` śledzi UTC serwera, zawsze 1×; `time_scale` może mieć inną datę nawet przy 1×.
Zmiana skali najpierw kotwiczy bieżącą datę. Skala 0 zatrzymuje wyłącznie środowisko.
Fizyka 20 Hz, snapshoty 10 Hz, cooldowny, timeouty i autosave zachowują realny czas.
Zegar nie zmienia manifestu, seedów, generator digest, world content revision ani geometrii.

Runtime RealTime porównuje SystemTime co 5 s; różnica ≥250 ms tworzy nową kotwicę/revision.
Naturalne dojście do granicy zakresu daje jawny fault i pauzę TimeScale=0 na ostatniej poprawnej próbce.
Błąd startup/odtwarzania blokuje start bez nadpisania save. Wyczerpanie revision jest błędem, nie wrap.

## 3. Konfiguracja i polecenia

Mutacje są dostępne przez istniejący uwierzytelniony kanał admin/terminal:

| Polecenie | Semantyka |
| --- | --- |
| `time status` | Odczyt UTC/mode/rate/revision/policy/fault. |
| `time mode real_time` | Bieżące UTC i 1×. |
| `time mode time_scale` | Zachowuje bieżącą datę i obecną skalę TimeScale lub 1× z RealTime. |
| `time set RFC3339_UTC` | Ustawia datę, przełącza TimeScale; zachowuje wcześniejszą skalę TimeScale. |
| `time scale DECIMAL` | Kotwiczy bieżącą datę, przełącza TimeScale; 0 jest pauzą. |
| `time offline pause\|advance` | Zmienia politykę restartu, bez zmiany bieżącej daty/mode/rate. |

Skala: dziesiętne cyfry, opcjonalna kropka i do 6 cyfr ułamka, bez float parsera.
Data: RFC3339 z Z lub +00:00, 0–3 cyfry milisekund, bez sekundy 60 i bez cichego zaokrąglania.
Błędna mutacja jest atomowo odrzucana. Każda przyjęta mutacja zwiększa revision.

`--environment-config PATH`: RON schema 1, maks. 16 KiB, `deny_unknown_fields`.
Wyłącznie inicjalizacja nowego świata. Istniejący save plus jawna opcja daje konflikt z instrukcją użycia admin commands.
Domyślnie RealTime/1×/Pause; mutable czas nie trafia do `world.planet.ron`.

Klient ma odczytowe `time status` oraz `sky info` i `sky quality low|standard`.
Zwykły gracz nie otrzymuje prawa do zmiany globalnego zegara.

## 4. Zapis v8

Jeden rekord CLOCK po WORLD, przed SECTOR/graczami, w tym samym atomowym snapshotcie:

    CLOCK\t1\t<revision:u64>\t<mode:u8>\t<world_utc_ms:i64>\t<saved_wall_utc_ms:i64>\t<rate_ppm:u32>\t<offline_policy:u8>

Sekwencja `\t` powyżej opisuje rzeczywisty separator tabulacji.
V8 wymaga dokładnie jednego poprawnego rekordu; brak/duplikat/zła kolejność lub zakres są błędem.
Limit całego snapshotu pozostaje 16 MiB. Worker serializuje próbkę pobraną na wątku właściciela stanu;
nie odczytuje zegara ponownie po oczekiwaniu w kolejce.

| Przypadek | Odtworzenie |
| --- | --- |
| V2–v7 | Dotychczasowe reguły migracji + nowy RealTime bieżącego UTC, revision 1. |
| V8 RealTime | Aktualne UTC; zachowany numer revision, nowy nonce i kotwica monotoniczna. |
| V8 TimeScale/Pause | Dokładnie data snapshotu i zapisana skala. |
| V8 TimeScale/Advance | Data snapshotu + max(0, teraz_wall − zapis_wall) × skala. |

Cofnięty wallclock daje zerowy przyrost offline i jawne ostrzeżenie.
Zakres/overflow przy odtworzeniu blokuje start, pozostawiając poprzedni plik.
Migracja nie usuwa danych graczy/sektorów, nie zmienia generatora ani promienia i nie nadpisuje save przy samym odczycie.

## 5. Protokół v12

Nagłówek 4 B, payload maks. 4096 B, jawne little-endian; v11/v12 są niekompatybilne.
ClockSample i EnvironmentDescriptor są dopisywane na końcu Welcome, TransitionPrepare i TransitionCommitted.

| Element | Pola / payload |
| --- | --- |
| ClockSample, 54 B | clock_id:u64, revision:u64, nonce[16], server_monotonic_ms:u64, world_utc_ms:i64, mode:u8, rate_ppm:u32, offline_policy:u8. |
| Descriptor, 49 B | clock_id:u64, profile:u8, astronomy_model_revision:u32, catalogue_id:u32, catalogue_sha256[32]. |
| Suffix, 103 B | ClockSample + Descriptor. |
| TimeProbe 0x0c, 12 B | seq:u32, t1:u64. |
| TimeProbeReply 0x8f, 44 B | seq:u32, echo_t1:u64, t2:u64, t3:u64, nonce[16]. |
| EnvironmentSync 0x8e, 135 B | WorldContext 32 B + ClockSample + Descriptor. |

Kody i wersje są wolne w bazie; równoległa kolizja wymaga aktualizacji kontraktu przed implementacją.
Nie zmieniać semantyki Ping/Pong ani Snapshot.server_time_ms (uptime).
Przeliczyć maksymalne rozmiary/prealokacje dla wszystkich wariantów.
Desktop, terminal i loadtest są obowiązkowymi konsumentami v12.

Nie wysyłać środowiska przed Welcome. Okresowy sync co 5 realnych sekund; mutacja wysyła najnowszy stan
w najbliższym outbound, z konsolidacją kolejki. Próbka przy commit jest świeża, nie zachowana z prepare.
Descriptor celu jest pending do atomowego zatwierdzenia świata; ClockId pozostaje wspólny.
Tożsamość modelu/katalogu jest sprawdzana przed gotowością wejścia, także w Interior.

## 6. Estymator klienta

`t1` = zakolejkowanie sondy klienta, `t2` = pełne zdekodowanie przez serwer,
`t3` = serializacja/zakolejkowanie odpowiedzi, `t4` = pełne zdekodowanie odpowiedzi klienta.
t3 nie oznacza fizycznego wysłania przez kartę sieciową.

    RTT = (t4-t1) - (t3-t2)
    offset = ((t2-t1) + (t3-t4)) / 2
    estimated_server_ms = local_monotonic_ms + offset

Offset to różnica zegarów monotonicznych, nie UTC. Arytmetyka signed; zachować pół milisekundy.
Nie używać wallclock klienta do sesji sieciowej.

- Jeden in-flight; timeout 2 s.
- Bootstrap nie częściej niż 1/s do 4 poprawnych; następnie co 5 s.
- Filtr maks. 8 poprawnych próbek, ważność 60 s, wybór najmniejszego RTT; remis wygrywa nowsza.
- Serwer przyjmuje maks. 2 sondy w przesuwanym oknie 1000 ms, bez nieograniczonych odpowiedzi.
- Gap poll >10 s: reset filtra/in-flight i bootstrap, bez odrabiania zaległych sond.
- Stale po >15 s bez poprawnego probe lub ClockSample; Unavailable po >60 s.
- Wygładzany jest bias prezentacji, nie sama data przez sumowanie dt. Stała 2 s, limit korekty 10% prędkości;
  maks. 30 realnych sekund na jedną korektę, potem jawny resync. Dokładna formuła/progi: [E004-05 / #60](https://github.com/clickbytes/minimium/issues/60).
- Nowa revision i nowa sesja są stosowane od razu. Portal nie zeruje offsetu wspólnego zegara.
- Brak danych/niezgodny zakres daje Unavailable; demo ma osobne LocalDemo.
- Jeden snapshot czasu/descriptor na klatkę jest wspólny dla nieba, gwiazd, Moon, świateł i globusa.

## 7. Obserwator i astronomia

Osie gry: +X Greenwich/równik, +Y północ, +Z 90°E/równik.
`lat=asin(y/r)`, `lon=atan2(z,x)`; dane wejściowe z precyzyjnego Position3Mm, obliczenia f64.
Baza N/E/U ma N×E=U. Astronomy Engine HOR (N,W,U) mapujemy `world=N*x−E*y+U*z`.
Istniejące odbicie X w finalnym widoku Earth stosujemy raz, także do nieba.

Earth 650 km/100 promili: fizyczna wysokość oka jest rekonstruowana z surowego GEBCO i reszty
pozycji gry. Oko/skok pozostają w skali 1 m; ocean używa powierzchni morza, nie dna.
Dokładny wzór i błędy: [E004-06 / #56](https://github.com/clickbytes/minimium/issues/56). Fizyczna wysokość MSL jest jawnym przybliżeniem bez modelu geoidy.
Miniaturowy promień pozostaje wyłącznie geometrii i masce wizualnego horyzontu.
EarthLike używa nazwanego mapowania ziemskiego nieba; Interior zwraca brak ramki nieba.

Astronomy Engine C v2.1.19, commit `61dc07020aaa6885d2c7f688a4d82beaf6edb9ef`:
lokalne vendored C/header/LICENSE, policzone SHA-256, statyczny build, wąski safe wrapper Rust,
jednowątkowy !Send/!Sync. Brak runtime fetch, zegara OS i wymaganego bindgen/libclang przy zwykłym buildzie.
Kompilator C jest jawną zależnością deweloperską; własny bridge i wrapper podlegają limitowi 500 linii.

`ut_days=(utc_ms−946728000000)/86400000`; TT wyprowadza biblioteka. UTC≈UT1 jest przybliżeniem.
API używa radianów, metrów i jednostkowych wektorów f64.
Geometryczne oznacza bez refrakcji atmosferycznej; pozorne = po jednej standardowej refrakcji.
GlobalSun jest geocentryczne; lokalne Sun/Moon topocentryczne; odrębna astronomia per shader jest zabroniona.

Dwie próbki orbitalne EQJ co 300 000 ms czasu symulowanego, interpolacja kartezjańska,
reuse wspólnego końca przedziału. Obserwator/obrót/topocentryka aktualne co klatkę.
Skok daty liczy tylko aktualny bracket, bez pętli nadrabiania.
Ruch własny 500 gwiazd aktualizowany po zmianie doby UTC oraz przy skoku/revision/katalogu;
wspólna macierz nie zastępuje ruchu własnego.

Moon phase z Moon→Sun i Moon→observer: `fraction=(1+cos(alpha))/2`.
Wizualna tarcza używa minimalnego obrotu geometric→apparent także dla światła/bieguna;
fazę liczymy przed refrakcją. Geocentryczne V magnitude backendu jest wejściem modelu względnej jasności.

## 8. Katalog gwiazd

Źródło: HYG 4.1, commit `c7f7f883fe678cc7680169a50ccd7dcc49b060ce`,
`hyg/CURRENT/hygdata_v41.csv`, 33 932 548 B, Git blob `ba2dec4eb0f6768914c7fc1051258100214ddf84`.
Importer przyjmuje lokalny plik, weryfikuje pin i liczy rzeczywisty SHA-256. Nie pobiera sieci przy build/startup.

Selekcja: wyklucz Sol/id0, waliduj dane, sortuj po (Decimal apparent V magnitude, HYG id),
wybierz dokładnie 500. Jednostką jest rekord HYG; nie scalać składowych po nazwie/HIP/odległości.
Membership nie zmienia się z półkulą, datą ani widocznością. Polityki błędów i braków: [E004-08 / #58](https://github.com/clickbytes/minimium/issues/58).

Asset `crates/assets/data/sky/stars-v1.bin`: 96 B nagłówka + 500×64 B = 32 096 B, LE.
Nagłówek: magic MNSKY500, schema 1, record size 64, catalogue id1, count500, flags0,
epoch2000.0, source CSV SHA-256 i payload SHA-256.
Rekord: HYG id u32, HIP id u32, apparent V f32, B−V f32, position EQJ 3×f64 pc,
velocity 3×f64 pc/rok juliański.
Manifest zawiera piny, hashe, selection policy, uporządkowane ID, liczniki uzupełnień i polecenie regeneracji.
`SkyCatalogIdentity.sha256` identyfikuje cały binarny asset.

Licencja danych CC BY-SA 4.0 i attribution Astronexus/David Nash, osobno od kodu silnika.
Dane/generated mają ścisły wyjątek; importer/reader nadal ≤500 linii.
Serwer używa tylko lekkiego id/hash z assets, bez C. Reader waliduje katalog raz.

## 9. GPU i grafika

[E004-09 / #61](https://github.com/clickbytes/minimium/issues/61) jest właścicielem jednego EnvironmentFrame, CameraUniform, EnvironmentUniform i frame.wgsl.

| Layout | Kontrakt |
| --- | --- |
| Group0 binding0 | CameraUniform 208 B: viewProj, skyViewProj, inverseSkyViewProj, viewport. |
| Group0 binding1 | EnvironmentUniform 304 B: lokalne/globalne Sun, Moon, geometria/faza, RGB, horyzont, flags, macierz EQJ. |
| Group1 | Materiał / istniejąca tekstura globusa. |
| Group2 | Shadow depth, comparison sampler, ShadowUniform 128 B; szczegóły [E004-13 / #65](https://github.com/clickbytes/minimium/issues/65). |
| StarInstance | 32 B ×500 = 16 000 B, vertex instancing; jeden draw call. |

Sky używa tej samej orientacji widoku bez translacji; nie podlega far plane terenu.
Normalne wszystkich ścian cube skierowane na zewnątrz; nie włączamy globalnego cullingu przy odbiciu Earth.
Wspólny sky_common.wgsl odpowiada za niebo/transmisję/horyzont/tone mapping.
Sun/Moon są pozorne w obrazie, geometryczne jako kierunki światła.
Cała tarcza Moon zasłania gwiazdy; w dzień zawiera rozproszenie atmosfery przed tarczą.

Kolejność: shadow → sky → stars → Sun → Moon → geometria → marker overview → UI.
Sky/ciała nie zapisują depth; późniejszy terrain je zasłania.
Preferowana surface sRGB; brak takiej powierzchni oznacza jeden liniowy LDR offscreen Rgba8Unorm
oraz końcowy encode-sRGB pass. Blendowanie odbywa się liniowo, bez podwójnej konwersji.
Globus ma globalny geocentryczny Sun, nie lokalne nocne wygaszenie obserwatora.
Overview ma neutralne tło i terminator, bez topocentrycznych tarcz obserwatora.

## 10. Jakość, cienie i weryfikacja

`--sky-quality low|standard`, domyślnie standard.
Oba profile mają identyczną astronomię, katalog i fazę.
Standard: jedna mapa Depth32Float 1024², receiver 128 m, caster selection 256 m;
budżet maks. 16 dodatkowych sektorów wizualnych /16 MiB /2 instalacje na klatkę,
maks. 128 kandydatów, kolejka 16 oraz do 2 zadań generowania w toku.
Niekompletne pokrycie daje jawny brak mapy w tej klatce; nie używać starego cienia i nie ruszać kolizji.
Low wyłącza mapę. Fallback zasobów jest raportowany jako requested/effective quality z powodem.

Diagnostyka: czas/offset/RTT/revision, observer, geometryczne/pozorne kąty,
faza/rozmiar, hash/count, cache, CPU mikrosekundy, prawdziwe draw calls i oszacowanie pamięci.
Nie podawać CPU submission jako GPU time ani nie formatować logów per frame.

Końcowe progi dla zgodnych wejść: Sun/Moon >5° ≤0,05°, fraction ±0,002,
rozmiar kątowy ≤1%, interpolacja ≤0,001°, gwiazdy ≤0,1° w uzgodnionym modelu.
Błąd czasu sieciowego raportowany osobno; asymetria sieci nie daje gwarancji absolutnej synchronizacji.
Scenariusze i dowody: [epic-004-verification.md](epic-004-verification.md), [E004-16 / #68](https://github.com/clickbytes/minimium/issues/68).
Testy/runtime są odroczone do wyraźnego uruchomienia końcowej fazy przez użytkownika, zgodnie z AGENTS.md.
