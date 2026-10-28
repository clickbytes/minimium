# Epic 002 — ownership i kontrakty

Pełny zakres, implementacja krok po kroku, kryteria odbioru i blokery są w issues #15–#30 pod Epic #14. Ten plik rejestruje wzajemne granice i kolejność, a nie duplikuje całej specyfikacji.

Końcowa faza runtime została wykonana 2026-10-28; aktualne wyniki i dowody są w [rejestrze weryfikacji](epic-002-verification.md), który ma pierwszeństwo przed niższymi notatkami „odroczone” zapisanymi podczas implementacji. Uruchomione scenariusze potwierdziły między innymi lane 1/2/auto dla 32 klientów, zapis/odzyskiwanie, lifecycle, mapę, wolną odpowiedź admina i wszystkie 18 workloadów. Testy jednostkowe wymuszają też niezależnie limity 64 KiB i 256 ramek oraz ścieżki częściowego i zerowego zapisu. Nadal nierozstrzygnięte są kontrola wizualna, osiągnięcie limitu bajtów pod rzeczywistym obciążeniem sieciowym oraz cel Pentium 500 MHz.

| Issue | Właściciel główny | Obszar / output |
| --- | --- | --- |
| #15 E002-00 | docs/plan/epic-002*, PR #13 | baseline, working branch, statyczny przegląd |
| #16 E002-01 | crates/core/src/{ids,identity,world} | PlayerId/ProfileId newtypes |
| #17 E002-02 | crates/protocol/src/* | typowane error/kinds, binary v8 |
| #18 E002-03 | crates/core/src/{admin_client,commands,endpoint,files,identity,server_lock} -> crates/platform | filesystem/admin/profil OS I/O |
| #19 E002-04 | crates/core/src/world/{map,obj}.rs -> crates/assets | map/OBJ loader i dane domenowe |
| #20 E002-05 | crates/core/src/world/*, core/executor.rs -> apps/server | pure simulation, batches, merge |
| #21 E002-06 | apps/server/src/{main,runtime*} | ServerRuntime, grouped state and phases |
| #22 E002-07 | apps/server/src/{connection,accept,receive,send,network,replication} | bounded TCP, fairness, lifecycle |
| #23 E002-08 | apps/server/src/{message_dispatch,world_inputs,commands} | admission, ordered world inputs, outcomes |
| #24 E002-09 | apps/server/src/storage/*, startup/lifecycle | save v3, owned shutdown |
| #25 E002-10 | apps/client/src/{session,connector,bootstrap,terminal_client} | ClientConnection/Owned-Transferred |
| #26 E002-11 | apps/client/src/renderer/*,console/* | component GPU/gameplay/input/UI |
| #27 E002-12 | client options/commands, server config, ctl, loadtest config | scoped typed config and commands |
| #28 E002-13 | telemetry/*, loadtest/*, metrics | phase metrics, report v2 |
| #29 E002-14 | docs/*, scripts/check_architecture.py | true dependency graph, ADR, audits |
| #30 E002-15 | integration PR, docs/plan/epic-002-review.md | complete verification and final MERGE |

## Obowiązkowe reguły przepływu

- Core nie używa std::fs/net/process, telemetry, GPU, admin, include sample. Protocol -> core (wyłącznie typy), platform -> core, assets -> core. Apps komponują komponenty.
- Typy domenowe w core, typy wire i error codes w protocol, typy CLI w swoim scope. NIE globalny enums.rs.
- World mutuje tylko server main lane; network/dispatch nie mutują bezpośrednio świata. Input apply order -> step -> replication, persisted snapshot captured consistently.
- W serwerze przechowywać bounded queue limits, partial write, disconnect-index updates; nie gubić EOF/handshake.
- ClientSession (TCP) i connector (DNS/process) są odrębne od GPU/renderer. Local Owned nie równa się Attached.
- Storage w module serwera może pozostać modułem; wydzielamy crates tylko przy realnych zależnościach.
- Każde issue aktualizować o status, PR, SHA, rzeczywiste zmienione pliki, wynik review, znane regresje oraz dowody statyczne. NIE odznaczać testów, których nie było.
- Zmiany w szeroko wspólnych plikach Cargo / lib.rs / docs/architecture.md synchronizować sekwencyjnie po merge każdego zależnego PR.

## Bramka końcowa

[#30](https://github.com/clickbytes/minimium/issues/30) wymaga kompletnego przeglądu wszystkich tasków i finalnej weryfikacji po jawnym poleceniu użytkownika zgodnie z AGENTS.md. Dopiero po tym wykonujemy końcowy MERGE epic branch do main, aktualizujemy merge SHA i zamykamy Epic #14.

## E002-00 — odbiór

PR #13 scalono jako a3cba16f232a7967505376f84497350667f5c38e; zmienione pliki: apps/client/src/main.rs, apps/server/src/{main,message_dispatch,startup,world_inputs}.rs, crates/core/src/world.rs, crates/core/src/world/world_state.rs, docs/plan/architecture-review-2026-10-28.md. Przegląd i kontrole opisano w baseline; scenariusze runtime odroczone. Dokumentacja baseline i macierz uzupełnione w PR #32; wykonano przegląd zmian.

## E002-01 — identyfikatory

`core::ids::PlayerId` obejmuje wszystkie wartości `u64` bez dodatkowej walidacji; `new` i `get` jawnie przekraczają granicę. Indeksy świata, snapshoty domenowe, wejścia, połączenia i store używają tego typu. Indeks połączenia pozostaje `usize`; licznik przydziału, fingerprint i czas pozostają `u64`. Zapis i odczyt snapshotu zachowują dziesiętną reprezentację i dotychczasową walidację dodatniego ID. `PlayerProfileId` pozostaje w `core::identity`; protocol reexportuje długość z tego źródła, a klient i generator konwertują jawnie przy Identify.

`claim` nadal blokuje aktywny profil, `release` usuwa aktywne ID, a `pending_departures` blokuje ponowne dołączenie do zastosowania Leave. Zamiana gracza w świecie usuwa stary wpis komórki. Nie wprowadzono `PlayerName`: transport i store wymagają ASCII alnum lub `_`, 1–24 bajtów, natomiast `World::rename_player` przycina do 24 znaków bez walidacji; ujednolicenie zmieniłoby zachowanie. Fingerprint pozostaje `u64`, ponieważ jest wartością mapy, a nie ID gracza; dalsze typowanie wymaga odrębnej oceny.

Granice pozostające dla E002-02: `ClientMessage::Identify` używa 16 bajtów; `ServerMessage::Welcome.id`, `ShotResult.target_id`, `Chat.sender_id`, `RemotePlayer.id` i `RemotePlayerIds` używają `u64`. Serwer konwertuje przy budowaniu wiadomości i odczycie/zapisie snapshotu; klient nadal konsumuje model wire. Zmiana typów kodeka musi zachować wszystkie wartości i bajty v8. Scenariusze rejoin i kompatybilności wire/save są odroczone do końcowej fazy testów.

## E002-02 — kodek v8

Modele protokołu używają `PlayerId`, `PlayerProfileId` i `ServerErrorCode`; konwersje do bajtów są w kodeku. `ClientKind` i `ServerKind` określają rodzaje ramek, nieznany kind nadal daje `UnknownMessage` po odebraniu pełnej ramki. Nieznany kod błędu jest zachowywany jako `Unknown(u8)`. Wspólne predykaty walidują nazwy, chat i składowe kierunku; dotychczasowa asymetria zdrowia `ShotResult` pozostała opisana w dokumentacji protokołu. Protocol nie używa telemetry bezpośrednio, ale obecna zależność protocol → core → telemetry pozostaje do E002-05; nie należy uznawać docelowego grafu za już osiągnięty. Scenariusze round-trip wszystkich kodów, ramek częściowych i zgodności bajtów są odroczone.

## E002-03 — platform

`minimium-platform` przejmuje admin_client, commands, endpoint, files, server_lock oraz operacje profilu w identity_store; core zachowuje typ i konwersję hex. Konsumenci korzystają z nowych ścieżek. Ładowanie map i OBJ nadal wykonuje I/O w core i zostaje przeniesione w E002-04, więc warunek całkowitego braku I/O w core nie jest jeszcze spełniony. Executor i telemetry pozostają do E002-05.

Przegląd unsafe: `MoveFileExW` otrzymuje żyjące podczas wywołania, zakończone zerem bufory UTF-16, sprawdza wynik i zwraca błąd OS; `BCryptGenRandom` zapisuje do poprawnego bufora, używa systemowego RNG i sprawdza status. Istniejące rzutowanie długości `usize` na `u32` dotyczy obecnych wywołań o rozmiarach 16 i 32; publiczny interfejs generyczny dla większych buforów wymaga osobnej ostrożności. Redakcja tokenu w Debug, lock i kolejność publikacji nie zostały zmienione. Runtime oraz współbieżne tworzenie profilu pozostają odroczone.

## E002-04 — assets

`minimium-assets::{load_map, parse_map}` jest jedynym parserem; zależy od core, które posiada walidowany model `WorldMap::from_parts`, geometrię i niezmieniony algorytm fingerprintu. `spawn()` zastępuje publiczne pole zapobiegając zmianie danych bez przeliczenia hash. Parser zachowuje limity i kolejność rekordów, kanonikalizację ścieżek OBJ oraz algorytm triangulacji. Core nie zawiera już plikowego I/O ani ścieżek map; executor/telemetry pozostaje do E002-05. Porównanie wartości fingerprintu i scenariusze błędnych ścieżek, symlinków, geometrii i limitów odroczono do końcowej fazy.

## E002-05 — symulacja i executor

Executor i przypisanie faz telemetrii należą do serwera; core nie ma zależności telemetry. `World::step_players_with` utrzymuje wyłączny dostęp do świata przez przygotowanie, dispatch i zastosowanie wyników; `step_batch` otrzymuje niemutowalny indeks kolizji. Dispatcher musi zwrócić wszystkie batche w kolejności wejściowej; serwerowy executor odtwarza tę kolejność po zakończeniu workerów. Próg 32, rozmiar batcha 16, copy-on-write przed dispatch i ponowne użycie buforów zachowano. Porządek HashMap nie jest gwarantowany między uruchomieniami. Deterministyczna symulacja oraz kontrolowany test sieciowy 32 graczy, 100 zmian wejścia i lane 1/2/auto dały bitowo identyczne pozycje, prędkości i grounded; koszty alokacji pozostają nieweryfikowane.

## E002-06 — runtime serwera

`ServerRuntime` posiada `GameState`, `ConnectionState`, `OperationsState` i `Scheduler`; fazy i kolejność udokumentowano w architecture.md. Świat pozostaje mutowany tylko w runtime na głównej lane. Błędy poll admin i accept przechodzą przez istniejące kontrolowane shutdown, guard endpointu jest zwalniany przed blokadą katalogu. Szerokie konteksty wejść usunięto w E002-08. Scenariusze shutdown i kolejności nie były uruchamiane.

## E002-07 — połączenia

`ConnectionPhase` obejmuje AwaitIdentify, Active z nierozdzielną parą ID/profil oraz Draining z zachowaną opcjonalną tożsamością, terminem i powodem. Przejściowa flaga EOF pozwala najpierw zastosować już odczytane ramki; `begin_close` jest idempotentne. `Client` posiada bufor wyjścia i rozlicza zapis częściowy; limity read/write i rotacja pozostały niezmienione. `swap_remove` nadal usuwa stary indeks i aktualizuje przeniesionego gracza, a Leave blokuje rejoin do zastosowania. Baseline repliki zmienia się po sukcesie kolejki. EOF, reconnect i backpressure ze zmieniającymi pozycje 64 klientami przeszły runtime; przy maksymalnej kolejce 36 776 B serwer odrzucił 732 snapshoty. Jednostkowe testy kolejki niezależnie wymuszają limity bajtów i ramek; test writer-a wymusza częściowy zapis, `WouldBlock` i `WriteZero`. 512-klientowa próba z jednym wolnym odbiorcą nie wytworzyła presji na loopback (maksymalna kolejka 616 B), więc wejście w byte-cap guard przy realnym ruchu sieciowym nadal nie jest potwierdzone.

## E002-08 — pipeline wejść

Admission jest metodą runtime i nie mutuje świata. Jedna kolejka `WorldInput` zachowuje rezerwację miejsca na Leave; faza apply przetwarza elementy kolejno i natychmiast przekazuje typowane `WorldEvent` do outbound przed następnym wejściem. Bufor kolejki jest odzyskiwany, a Move nie tworzy zdarzenia ani nowej alokacji. Outbound sprawdza zgodność ID z indeksem połączenia; w razie niespójnego indeksu pomija adresata zamiast wysłać wiadomość innemu graczowi. `Accepted` dla nazwy oznacza przyjęcie do kolejki, nie zastosowanie lub zapis. Usunięto szerokie `DispatchContext` i `WorldInputsContext`; cooldowny i liczniki pozostają w dotychczasowych fazach. Scenariusze kolejności, reconnect i potwierdzeń są odroczone.

## E002-09 — zapis graczy

Indeksy i aktywne claims `PlayerStore` są dostępne przez metody; worker przechowuje typowany status z zachowaniem tekstu CLI. Rozłączenie kanału completion przy oczekującym zapisie staje się widocznym błędem. Przy shutdown błąd runtime pozostaje główny, a błędy finalnego zapisu i cleanup są zgłaszane dodatkowo; jeśli runtime nie zawiódł, pierwszy błąd shutdown staje się wynikiem. Format v3/v2, 256 shardów, kanał o pojemności 1, sortowanie na workerze i atomowe replace nie zostały zmienione. Awaria dysku, worker panic, migracja, utrata procesu i spójność zapisu pozostają scenariuszami odroczonymi.

## E002-10 — kontroler klienta

Terminal i renderer używają `ConnectionController`, który posiada connector, sesję, attempt i guard lokalnego serwera. `ClientSession` zachowuje handshake, bufory i heartbeat; oba frontendy pozostają adapterami zdarzeń. Spóźniony wynik lokalnego workera wraca do niego w celu sprzątania, a zastępowany owned guard jest sprzątany przez śledzony worker; następna próba czeka na zakończenie sprzątania. Attached nie zatrzymuje cudzego procesu. DNS/cancel, mismatch, heartbeat, rehost, błędy okna i cleanup są odroczone do końcowej fazy.

## E002-11 — renderer

Renderer składa właścicieli GPU, gameplay, input, UI, metryk, ustawień i kontrolera połączenia. Konstrukcja, resize i draw GPU nie obsługują connectora ani komend; predykcja, snapshot i interpolacja należą do gameplay, a kierunek ruchu do input. Orkiestracja klatki i adapter zdarzeń pozostają w Renderer. Geometria zdalnych graczy jest aktualizowana po zmianie/interpolacji, bufory i atlas pozostają ponownie używane. Ocena wizualna, glyphs, resize, predykcja i scenariusze dwóch klientów są odroczone.

## E002-12 — konfiguracje i komendy

Katalog klienta przypisuje typowany identyfikator do każdej nazwy i aliasu; parser, help i completion korzystają z tej samej tabeli. `ServerCommand::ALL` i `name` są źródłem parsera i tekstu help serwera/ctl. Typy lane pozostają osobne: zakres i znaczenie server, client i generator różnią się; tabela flag, domyślnych wartości, duplicate i konfliktów znajduje się w commands.md. Istniejący nieprecyzyjny komunikat serwera o niepoprawnym `--threads` opisano bez zmiany zachowania. Nie uruchamiano CLI; scenariusze parserów, help, aliasów i błędów są odroczone.

## E002-13 — telemetria i raport

Inwentaryzacja metryk, granice histogramów, opcjonalna instrumentacja i potencjalne koszty alokacji znajdują się w performance.md; format v2 i ograniczenia identyfikacji endpointu w scenarios/report-v2.md. Ranga percentyla używa szerszej arytmetyki, a niefinitywna opcjonalna liczba jest zapisywana jako brak wartości. Statycznie sprawdzono i uruchomiono 18 manifestów; raporty v2, pola tokenów i rezultaty lane zostały sprawdzone. Kontrola sieciowa 32 graczy dała identyczny stan dla lane 1/2/auto. Koszty alokacji nadal wymagają odrębnego pomiaru.

## E002-14 — granice architektury

Rzeczywisty graf i przyszły układ rozdzielono w architecture.md, decyzje opisano w ADR 0004, a dowody i odroczone scenariusze w epic-002-review.md. Checker analizuje krawędzie pakietów workspace i zgłasza zakazane zależności core oraz serwera od grafiki; nie bada tranzytywnych zależności zewnętrznych. Scoped search nie wykazał I/O ani unsafe w core. Przegląd statyczny nie dowodzi nieobecności wszystkich martwych API ani poprawności runtime.

## E002-15 — końcowa integracja i weryfikacja

Końcowy przebieg runtime potwierdził też replikację snapshotów: klient obserwujący dostał pełny snapshot początkowy, deltę dołączenia drugiego klienta i deltę usunięcia go po Quit. Drugi klient otrzymał Bye. Log i wynik: `D:\Temp\minimium-epic002-snapshot-transitions-final\result.json`.

Wyniki bramek statycznych i końcowe scenariusze runtime opisuje epic-002-verification.md. Wszystkie 18 manifestów obciążenia, dwa klienty, ownership Attached/Owned, lock, zapis/restart, migracja v2, wymuszony błąd zapisu z odzyskaniem poprzedniego snapshotu, limity i wolna odpowiedź admina oraz przejście po geometrii demo przeszły na hoście x86_64. Test symulacji i kontrolowany test sieciowy 32 graczy porównały lane 1/2/auto; pozycje, prędkości i grounded były bitowo identyczne w próbce końcowej. Wolni klienci wywołali 732 snapshot drops przy maksymalnej kolejce 36 776 B. Dwa testy jednostkowe wymuszają niezależnie limity 64 KiB i 256 ramek; rzeczywiste obciążenie sieciowe nie dotarło do limitu bajtów. Testy writer-a pokrywają partial write i WriteZero bez wymuszania syscall-a TCP. Nie wykonano pełnego testu wizualnego, a sprzęt Pentium 500 MHz pozostaje niedostępny. PR #31 nadal wymaga końcowego review przed merge.
