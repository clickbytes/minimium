# Epic 003 — Odtwarzalne światy: planeta, sektory i dungeon

## Cel

Jeden wersjonowany system światów i cyklu życia sektorów, z odrębnymi instancjami
planety i dungeonu. Demonstrator prowadzi od proceduralnej planety przez wejście i
kilka sal dungeonu z powrotem na planetę; zapis i reconnect odtwarzają lokalizację.

`core` przechowuje czysty model, niezmienniki, symulację i kontrakty sektorów.
`procgen` dostarcza czyste, wersjonowane generatory z losowością Philox4x32-10,
oddzielnymi kanałami i kwantyzowanym wynikiem. `assets` waliduje manifesty i
definicje. `server` jest jedynym właścicielem członkostwa, instancji, transferu i
zatwierdzania zmian. `platform` udostępnia serwerowi porty zapisu, a `client`
zarządza sceną, predykcją i zasobami renderera.

## Baza i audyt prototypu

- Bieżący szczyt gałęzi integracyjnej 002: `957594de7619495e3476d9d388d246c73f72b82c`.
- Lokalny `main`: `53314514dac1456744af909aa4a1ea910458d111`.
- PR #31 z integracją 002 jest nadal otwarty jako draft, skierowany do `main`;
  GitHub zgłasza czysty merge, ale brak recenzji i skonfigurowanych kontroli.
- Kontynuację lokalną oparto na szczycie 002, aby nie budować świata na starszym
  `main`. Przed końcową integracją trzeba zachować ten łańcuch bazowy.

PR #49 (`feature/cube-sphere-planet-dungeon`) jest otwarty i opisuje pierwszy
przekrój gry. Audyt jego diffu: [PR #49](https://github.com/clickbytes/minimium/pull/49).

Do adaptacji jako pomysły: ograniczona tessellacja cube-sphere, orientacja lokalna
z radialnym up, ograniczenia geometrii i walidacja bezpiecznych pozycji celu.
Wersja 1 projekcji musi jednak zostać jawnie zdefiniowana przez nowy kontrakt.

Do odrzucenia jako kontrakt docelowy: jeden `WorldMap` z regionami planety i
dungeonu, wspólny układ współrzędnych, `WorldKind` wybierany przez odległość od
sfery, teleport gracza wewnątrz tej samej symulacji, wspólny indeks graczy,
rozszerzenie fingerprintu mapy v3, zachowanie protokołu v8 i zapisu v3 jako modelu
świata. Prototyp sam wskazuje brak instancji, sektorów, streamingu, rewizji
generatora i trwałego kontekstu członkostwa.

## Kontrakty

- Seed, rewizje, definicje, instancje, lokalizacje, adresy sektorów i pochodzenie
  obiektów są typami domenowymi. Lokalizacja zawsze zawiera instancję, kwantyzowaną
  pozycję oraz orientację.
- `SectorKey` rozróżnia stały adres twarzy planety od lokalnych współrzędnych 3D
  dungeonu. Adres renderowanego LOD i komórka kolizji są odrębnymi typami.
- Krawędzie i narożniki cube-sphere są wersjonowanym kontraktem. Zmiana projekcji
  lub generatora wymaga nowej rewizji świata.
- Sektorowa infrastruktura współdzieli limity żądań, ticket, cache, instalację,
  zapis i zwalnianie. Sąsiedztwo oraz wybór sektorów pozostają topologiczne.
- Serwer jest autorytatywny. Każda instancja ma jednego właściciela mutacji.
- Protokół v9 przenosi instancję, rewizję zawartości, `membership_epoch` i
  `TransitionId`. Niezgodny digest blokuje wejście do świata.
- Snapshot gracza zapisuje instancję i punkt powrotu. Dane v2/v3 pozostają
  odizolowanym światem legacy i nie są przypisywane do nowej planety.
- Testy i uruchamianie aplikacji pozostają odłożone do osobnego, wyraźnego
  polecenia rozpoczęcia końcowej fazy testowej. Faza została rozpoczęta poleceniem
  użytkownika „dokoncz caly epic”.

## Etapy

| Etap | Rezultat |
| --- | --- |
| E003-00 | Baza po 002 i audyt PR #49; wybrać przenoszone pomysły. |
| E003-01 | Manifest, definicja, instancja, lokalizacja, rewizje i adresy sektorów. |
| E003-02 | `procgen`, specyfikacja, Philox4x32-10, kanały i stabilne ID obiektów. |
| E003-03 | Topologia cube-sphere v1, sąsiedzi, krawędzie, narożniki i klucze LOD. |
| E003-04 | Kwantyzowany teren planety, bezpieczny spawn, wejście i dojście. |
| E003-05 | Ograniczony plan BSP dungeonu, sale, korytarze i materializacja sektorów. |
| E003-06 | Bounded lifecycle sektorów, tickety, cache, kolizje i polityki selekcji. |
| E003-07 | Ruch planetarny i dungeonowy, orientacja i kapsuła zgodne z geometrią logiczną. |
| E003-08 | `WorldRegistry` i izolacja stanu symulacji między instancjami. |
| E003-09 | Protokół v9, rewizje, identyfikator instancji i epoka członkostwa. |
| E003-10 | Transfer portalem: prepare, ready, commit na granicy ticka i snapshot celu. |
| E003-11 | Zapis zmian, lokalizacja, powrót i migracja v2/v3 do świata legacy. |
| E003-12 | Zmiana sceny i zasobów klienta bez odtwarzania okna, GPU ani TCP. |
| E003-13 | Demonstrator planeta → dungeon → planeta, zapis i reconnect. |
| E003-14 | ADR, dokumentacja, graf zależności, eksporty, `unsafe`, długość plików i statyczny audyt. |

Każdy etap kończy się przeglądem i osobnym PR do gałęzi integracyjnej epica.
Integracja do `main` następuje dopiero po bramkach i osobno rozpoczętej fazie
testowej.

## Rejestr pracy

| Etap | Zmiany i decyzje | Kontrole / uwagi |
| --- | --- | --- |
| E003-00 | Ustalono bazę 002 i przejrzano diff PR #49; wspólny świat prototypu nie jest kontraktem. | Audyt PR wykonany; nie zmieniono remote ani nie utworzono PR. |
| E003-01 | Dodano typy tożsamości świata, manifest, wersje i kwantyzowaną lokalizację; oddzielono adresy sektorów, kolizji i renderowania. | fmt, check, Clippy, graf zależności, limit linii i diff-check przechodzą; testy odłożone. |
| E003-02 | Dodano `minimium-procgen`, Philox4x32-10, stałe kanały oraz adresowany seed/revision/sector i stabilne ID obiektu. | Philox reference KAT, seek po indeksie, separacja kanałów i powtarzalne ID przechodzą na Windows; innych platform nie wykonano. |
| E003-03 | Zamrożono kolejność i bazy ścian v1; dodano sąsiadów przez krawędzie, sektory narożników i wspólne całkowitoliczbowe klucze próbek. | Kanoniczne próbki krawędzi wszystkich ścian przechodzą test; osobny test narożników nie istnieje. |
| E003-04 | Dodano fixed-point smooth value noise na kanonicznych próbkach cube-sphere oraz deterministyczny spawn, wejście i dziewięć punktów dojścia. Portal runtime podłączono w E003-10. | Test sprawdza kwantyzację pozycji, bezpieczny spawn i ograniczoną różnicę wysokości punktów trasy; trasa planety do portalu przechodzi symulację. |
| E003-05 | Dodano ograniczony plan BSP dungeonu z pokojami, połączeniami korytarzami i materializacją piętra do lokalnych sektorów; ręczny plan ma walidator tego samego typu. | Piętro jest na y=0, ściany na kafelkach pustki, sektory 16×16×16; spójność kafli przez granice sektorów i fizyczna trasa połączonych sal przechodzą. Parser `assets` dodano w E003-14. |
| E003-06 | Dodano współdzielony selektor sąsiedztwa i slicer geometrii, ticketowane cache po obu stronach, instalację danych/kolizji i ograniczone zwalnianie. Serwer wstrzymuje ruch do gotowości otoczenia; klient tworzy kolizje z aktywnego otoczenia. | Testy sprawdzają limity, spóźniony ticket, gotowość kolizji, przypięcie i zwalnianie. Serwer zachowuje pełny indeks wyłącznie dla hitscan. |
| E003-07 | Dodano planetarny radialny pion, tangent-frame ruchu, podkroki, dokładny test zorientowanej kapsuły względem geometrii i stały pion dungeonu; kamera klienta używa tej samej ramy. | Testy kapsuły radialnej, odstępu od przeszkód i kontaktu z podłożem przechodzą. |
| E003-08 | `WorldRegistry` prowadzi osobny `World` i członkostwa z epoką na instancję; dodano routing zdarzeń i wejść według instancji. | Test z identycznymi współrzędnymi w różnych instancjach przechodzi. |
| E003-09 | Wprowadzono protokół v9 z manifestem, fingerprintem mapy, kontekstem instancja/rewizja/epoch, snapshotami i odrzucaniem starego kontekstu. Welcome przekazuje też autorytatywną zapisaną pozycję reconnectu. | Round-trip ramek v9, odrzucenie starej epoki i nieznanego digestu przechodzą. |
| E003-10 | Dodano `prepare → ready → commit` na granicy ticka, anulowanie przed commit, reset predykcji i wymuszenie pełnego snapshotu celu. | Test sprawdza anulowanie, gotowość celu, commit i powrót do punktu zapisanego w transferze. |
| E003-11 | Zapis v6 utrwala manifesty i fingerprinty światów, lokalizacje i punkty powrotu graczy oraz rewizjonowane zestawy stanu obiektów w sektorach. Rejestr serwera waliduje topologię, rewizję monotoniczną i limity; v2/v3 migrują wyłącznie po fingerprintcie legacy. | Testy sprawdzają v6 save → load → claim profilu, migrację v2/v3, limit odczytu i atomowy zapis. Bieżący demonstrator nie ma interakcji zmieniającej stan obiektów; zmiany geometrii pozostają poza modelem. |
| E003-12 | Renderer utrzymuje bufory GPU dla sektorów aktywnego otoczenia. Transfer przygotowuje sektory docelowe i potwierdza gotowość dopiero po instalacji danych, kolizji i buforów GPU; scena przełącza się bez odtwarzania okna, urządzenia GPU ani TCP. | Źródłowe mapy pozostają w pamięci CPU do lokalnego wycinania sektorów. Sieć i reconnect po restarcie przeszły; płynność przejść i odbudowa zasobów GPU czekają na wizualny przegląd klienta. |
| E003-13 | Domyślne manifesty generują osobną planetę i dungeon połączone portalami; powrót wykorzystuje punkt zapisany przy commit. Welcome przesyła zapisaną pozycję reconnectu. | Demonstrator v9 po TCP przeszedł od planetarnego spawnu przez portal, wszystkie wygenerowane sale i portal powrotny. Po graceful stop i restarcie tego samego serwera profil wrócił do zapisanej pozycji planety. |
| E003-14 | Dodano ADR, kontrakty świata, parser ręcznego dungeonu i kontrolę grafu zależności. Uzupełniono wynikami fazy testowej. | Kontrole statyczne przechodzą. Recenzje i integracja do gałęzi epica/main pozostają otwarte. |

## Przegląd i ograniczenia bieżącego przyrostu

- `core` zachowuje brak lokalnych zależności; `procgen` zależy wyłącznie od `core`
  i nie wykonuje I/O, pracy równoległej ani operacji GPU.
- Kolejność i bazy ścian cube-sphere odpowiadają projekcji użytej w PR #49, ale
  adresowanie sektorów, próbek, kolizji i LOD ma osobne typy oraz stałą siatkę.
- Adres losowy jest 128-bitowym deterministycznym skrótem wejścia, nie hashem
  kryptograficznym. Kolizja identyfikatora obiektu jest teoretycznie możliwa;
  właściciel świata musi wykryć konflikt przy materializacji.
- `cargo check --workspace --locked --offline` i Clippy kończą się bez błędów.
  Clippy zgłasza duże warianty enum protokołu/sesji. Check zgłasza kopiowanie
  cache inkrementalnego, bo filesystem nie obsługuje hardlinków.
- Przegląd `unsafe` znalazł wyłącznie istniejące wywołania FFI i alokatora w
  `platform` oraz `telemetry`; nowe moduły epica nie wprowadzają bloków `unsafe`.
- Po wyraźnym poleceniu rozpoczęto końcową fazę testową. Wyniki i zakres są
  zapisane poniżej.
- Zapis v6 przechowuje kompletne, rewizjonowane zestawy logicznego stanu
  obiektów na sektor; rejestr odrzuca starsze rewizje, niezgodną topologię i
  przekroczenie limitów. Demonstrator nie tworzy zmian obiektów. Źródłowe mapy
  klienta pozostają w pamięci CPU do lokalnego wycinania sektorów.
- Przyrost jest commitowany na gałęzi epica i zgłoszony w PR #50 jako jeden
  skonsolidowany przegląd; osobnych PR-ów ani issue dla etapów nie utworzono.
  PR #50 bazuje na otwartym PR #31 dla E002 i nie jest jeszcze scalony do `main`.

## Kontynuacja implementacji

- Poprawiono dobór geometrii planetarnej do sektora: trójkąty są przypisywane
  według zakresów UV w kontrakcie ściany cube-sphere, a nie wyłącznie po
  sektorach wierzchołków. Obwiednie pudełek uwzględniają narożniki; selekcja
  ładuje także bezpieczne otoczenie sąsiadów przy krawędziach ścian.
- Klient sprawdza fingerprint mapy, rewizję zawartości i digest generatora
  przed potwierdzeniem gotowości transferu.
- Ruch serwera korzysta z indeksu kolizji złożonego z zainstalowanych sektorów;
  gracz jest wstrzymany do instalacji całego swojego otoczenia. Ticketowane
  sektory zainteresowania są przypinane, a zwalniane po opuszczeniu obszaru.
- Hitscan zachowuje pełny indeks statycznej mapy, aby niezainstalowane sektory
  nie usuwały przeszkód z odległych linii strzału.
- Serwer i klient współdzielą selekcję sąsiadów oraz wycinanie geometrii. Klient
  używa ticketowanego cache, sektorowego indeksu kolizji i buforów GPU dla
  aktywnego otoczenia; potwierdzenie transferu czeka na gotowość targetu.
- Zainteresowanie sektorem i bufory GPU uwzględniają rewizję generatora oraz
  treści; zmiana rewizji powoduje odrzucenie starych danych i odbudowę cache.
- Zmiana rewizji może zastąpić nieprzypięty sektor będący w trakcie generowania;
  późny wynik ze starym ticketem jest odrzucany. Dungeon używa jednego stałego
  rozmiaru sektora dla adresowania, selekcji sąsiadów i materializacji planu.
- Po anulowaniu transferu klient zwalnia zainteresowanie sektorami i bufory GPU
  celu. Przy zmianie członkostwa czyści lokalne wejścia i historię predykcji.
- Zapis v6 wiąże lokalizacje graczy i punkty powrotu z manifestami oraz
  fingerprintami każdej instancji, a także zapisuje ograniczone, rewizjonowane
  zestawy stanu obiektów sektorów. Wersje 4 i 5 zachowują dotychczasowe reguły
  przypięcia świata; v2/v3 migrują wyłącznie do pasującego świata legacy.
- Zapis odrzuca snapshot powyżej limitu odczytu 16 MiB przed atomową podmianą,
  pozostawiając poprzedni poprawny plik na miejscu. Serwer używa do odczytu i
  atomowego zapisu ograniczonych portów `platform`; walidacja formatu pozostaje
  w module storage serwera.
- Welcome przesyła serwerową pozycję odtworzoną z zapisu; klient instaluje scenę
  i sektory startowe od tej pozycji przy reconnect.
- Odtworzenie wykazało, że kwantyzacja milimetrowa planetarnego kontaktu z
  powierzchnią mogła umieścić kapsułę kilka milimetrów w geometrii. Zapis
  przesuwa radialną pozycję o 10 mm na zewnątrz przed kwantyzacją; test v6
  odtwarza pozycję z rzeczywistej trasy i sprawdza kolizję po wczytaniu.
- Pliki przekraczające limit 500 linii podzielono na moduły tematyczne. Test
  geometrii kapsuły wydzielono do `geometry/triangle_distance.rs`; kontrola
  długości plików przechodzi dla wszystkich 138 plików źródłowych.
- `cargo fmt --all -- --check`, `cargo check --workspace --locked --offline`,
  Clippy, kontrola grafu architektury, limitu linii i `git diff --check`
  przechodzą. Pozostałe ostrzeżenia Clippy są stylistyczne lub dotyczą dużych
  wariantów enum; kompilacja zgłasza kopiowanie cache inkrementalnego, ponieważ
  filesystem nie obsługuje wymaganych hardlinków.
- Pełny test workspace przeszedł: 33 testy jednostkowe, 0 błędów; wszystkie
  doc-testy również przeszły. Test obejmuje Philox KAT, kolejność żądań,
  kanoniczne krawędzie cube-sphere, spójność dungeonowych sektorów, limity i
  odrzucenie spóźnionego ticketu, izolację instancji, digest klienta, epoch,
  transfer anulowany i zatwierdzony, fizyczną trasę przez sale dungeonu, zapis
  v6, migrację v2/v3, limity odczytu, atomowy zapis i odtworzenie radialnego
  kontaktu z powierzchnią po kwantyzacji.
- Ręczny klient protokołu v9 wykonał po TCP planetarny marsz do portalu,
  prepare/ready/commit, traversal wszystkich 100 komórek trasy przez sale,
  powrót do planety oraz reconnect po graceful stop i restarcie serwera. Digesty
  manifestów zostały zweryfikowane; profil po restarcie wrócił do zapisanej
  planety. Próba ujawniła i pomogła naprawić kwantyzację lokalizacji radialnej.

## Wyniki końcowej fazy testowej

- `cargo test --workspace --locked --offline --message-format short`: 33 testy
  jednostkowych przeszło; wszystkie doc-testy przeszły.
- `cargo fmt --all -- --check`, `cargo check --workspace --locked --offline`,
  Clippy, `scripts/check_architecture.py`, `scripts/check_file_lengths.py` oraz
  `git diff --check` przechodzą. Clippy pozostawia ostrzeżenia o dużych
  wariantach enum (`ServerMessage`, `SessionEvent`). Rust ostrzega o kopiowaniu
  inkrementalnego cache zamiast hardlinków z powodu filesystemu.
- Przegląd `unsafe` nie znalazł nowych bloków w epiku; znalezione bloki należą
  do istniejących wywołań FFI i alokatora w `platform` oraz `telemetry`.
- Test w `world_registry` symuluje planetarny marsz do wejścia z aktualizacją
  sektorów, cancel i commit transferu, fizyczne przejście przez wszystkie
  wygenerowane sale/korytarze oraz powrót do zapisanej pozycji. Osobny test
  formatu sprawdza odtworzenie lokalizacji z v6, a test sesji sprawdza kontekst
  Welcome i odmowę nieznanego digestu.
- Sieciowy demonstrator v9 przeszedł planetę, transfer do dungeonu, wszystkie
  sale, powrót i reconnect po zatrzymaniu oraz ponownym uruchomieniu serwera.
  Nie wykonano wizualnego przeglądu renderowanej sceny podczas transferu.
- `origin/epic/002-architecture-stabilization` nadal jest bazą PR #31, który
  pozostaje otwartym draftem do `main` bez decyzji recenzenckiej i kontroli CI.
  PR #50 jest otwartym draftem tego epica na gałęzi bazowej E002. Zawiera
  skonsolidowany przyrost E003, nie osobne PR-y każdego etapu. Scalanie końcowe
  i zamknięcie epica czekają na integrację bazowego E002 oraz przegląd PR-ów;
  pozostał również wizualny przegląd renderowanego transferu klienta.
