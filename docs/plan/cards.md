# Karty refaktoru R00–R12

Zakres, kontrakty i kryteria odbioru opisuje [macierz](refactor-matrix.md). Każdy pakiet zależy od odbioru poprzedniego. Przed przydziałem podzadania koordynator zapisuje zamkniętą listę plików, właściciela, interfejsy, ograniczenia i kryteria; jeden plik ma jednego właściciela. Koordynator integruje manifesty, pętle i unsafe. Scenariusze wykonania pozostają odroczone.

| Karta | Cel | Status | Kontrakt przydziału |
| --- | --- | --- | --- |
| R00 | Baza, plan i karty | odebrane | Koordynator: AGENTS.md, refactor-baseline.md, refactor-matrix.md i cards.md. Dane użytkownika zachowane; fmt, check i Clippy przeszły. |
| R01 | Poprawki usterek | odebrane | Kontrakty i dowody w sekcji R01 poniżej. |
| R02 | Konfiguracja i stałe | odebrane | Kontrakty i dowody w sekcji R02 poniżej. |
| R03 | Protokół | odebrane | Kontrakt i dowody w sekcji R03 poniżej. |
| R04 | Świat i fizyka | odebrane | Kontrakt i dowody w sekcji R04 poniżej. |
| R05 | Serwer i storage | odebrane | Kontrakt i dowody w sekcji R05 poniżej. |
| R06 | Sesja klienta | odebrane | Kontrakt i dowody w sekcji R06 poniżej. |
| R07 | Komendy | odebrane | Kontrakt i dowody w sekcji R07 poniżej. |
| R08 | Renderer i UI | odebrane | Kontrakty i dowody w sekcji R08 poniżej. |
| R09 | Teksty i lokalizacja | odebrane | Kontrakty i dowody w sekcji R09 poniżej. |
| R10 | Telemetria | odebrane | Kontrakty i dowody w sekcji R10 poniżej. |
| R11 | Generator i raporty | odebrane | Kontrakty i dowody w sekcji R11 poniżej. |
| R12 | Kontrola struktury | odebrane | Kontrakty i dowody w sekcji R12 poniżej. |

Kryteria odbioru i scenariusze każdej karty odpowiadają jej wierszowi w macierzy oraz wspólnej kontroli i odroczonej weryfikacji pod tabelą. Gotowe oznacza gotowość do rozpoczęcia po spełnieniu zależności, nie zakończenie.

## R01 — przydziały

- Profil: `crates/core/src/identity.rs`, jeden wykonawca. Krótka blokada tworzenia, pełny plik tymczasowy i atomowa publikacja; format bez zmian, uszkodzony istniejący plik jest błędem.
- Klient: `apps/client/src/options.rs`, `apps/client/src/renderer.rs`, `apps/client/src/main.rs`, jeden wykonawca. Odrzucić konflikty demo oraz traktować zerowy zapis niepustego bufora jako zerwanie z zachowaniem rozliczenia postępu.
- Próbki: `apps/loadtest/src/report.rs`, `apps/loadtest/src/main.rs`, `docs/scenarios/report-v2.md`, jeden wykonawca. Trzy próbki należą do tego samego endpointu z tokenem tylko w pamięci; adres generatora musi być jawnym 127.0.0.1:PORT zgodnym z adresem nasłuchu statusu i portem endpointu; w razie niezgodności wszystkie próbki null, diagnostyka bez tokenu. Status udostępnia `game_address`.
- Koordynator: `apps/server/src/main.rs` i wymagane pliki serwera. Po 32 ramkach przerwać obsługę danego klienta i zachować pozostałe dane na następny obieg; `status` zawiera rzeczywisty adres nasłuchu.

Odbiór: statyczny przegląd wszystkich ścieżek, formatowanie i kompilacja/Clippy objętego zakresu; odroczone scenariusze: współbieżne utworzenie profilu i uszkodzony plik, częściowy i zerowy zapis, 32/33 ramki, konflikty opcji, podmiana endpointu oraz niezgodny adres. Format profilu, endpointu i pola raportu v2 pozostają zgodne.

Przegląd: bufor wejściowy serwera jest dekodowany przed następnym odczytem; limit kończy obieg bez rozłączenia. `write_all` terminala już obsługuje zerowy zapis; desktop rozłącza i zachowuje wcześniejsze liczniki. Profil jest ponownie odczytywany pod blokadą przed publikacją; blokada obejmuje współpracujące procesy. Próbki odrzucane są również przy braku możliwości potwierdzenia ciągłości endpointu. `cargo fmt --all -- --check`, `cargo check --workspace --locked --offline` i `cargo clippy --workspace --locked --offline` przeszły; ostrzeżenia pozostają zgodne z bazą. Scenariuszy nie uruchomiono.

## R02 — przydziały

- Serwer i wspólne nazwy: koordynator, `apps/server/src/main.rs`, nowy `apps/server/src/config.rs`, `crates/core/src/admin_client.rs`, `crates/core/src/server_lock.rs`, `crates/core/src/files.rs`, `crates/core/src/world.rs`, `crates/protocol/src/lib.rs`, `apps/ctl/src/main.rs`, `docs/commands.md`, `docs/architecture.md`. Typowane ustawienia, walidacja przed startem, nazwy plików przy właścicielach formatów; zachować domyślne wartości i zakresy.
- Generator: jeden wykonawca, `apps/loadtest/src/main.rs` oraz nowy `apps/loadtest/src/config.rs`. Wydzielić typowaną konfigurację i enum wyboru workerów, zachować istniejące CLI, domyślne wartości i zakresy; nie uruchamiać generatora.
- Klient: jeden wykonawca, `apps/client/src/options.rs`, `apps/client/src/bootstrap.rs`, `apps/client/src/main.rs`, `apps/client/src/renderer.rs`. Wprowadzić typowany wybór lane'ów bez zmiany CLI, domyślnych ścieżek i zakresów; konfigurację walidować na starcie. Bez zmian w innych plikach.

Odbiór: przegląd zgodności CLI i pojedynczej walidacji; formatowanie, kompilacja i Clippy. Dalsze scenariusze: błędne, brakujące, powtórzone oraz graniczne argumenty i lane'y 1/2/auto.

Przegląd: wydzielono konfigurację serwera i generatora, typowane wybory lane'ów/workerów, wspólne nazwy plików, domyślną mapę, heartbeat, limit nazwy protokołu i stałe częstotliwości serwera. Limit znaków w świecie ma odrębną semantykę od limitu bajtów protokołu. Adres bind jest rozwiązywany przed uruchomieniem storage, a help opisuje faktyczny zakres lane'ów 1–64. `cargo fmt --all -- --check`, `cargo check --workspace --locked --offline` i Clippy przeszły. Scenariuszy nie uruchomiono.

## R03 — przydział

Koordynator: `crates/protocol/src/lib.rs`, nowe moduły w `crates/protocol/src/` i `apps/server/src/main.rs` przy integracji limitu. Podzielić modele, limity, ramki, odczyt, kodowanie i dekodowanie. Utrzymać publiczną fasadę i v8 w tej samej kolejności i z tymi samymi limitami; maksymalny rozmiar wiadomości należy do protokołu. Nie dodawać alokacji tylko dla ostrzeżenia dużego enumu. Odbiór: przegląd eksportów i formatu, fmt/check/Clippy; odroczone scenariusze zgodności kodowania, dekodowania i limitów.

Przegląd: porównano poprzednie implementacje z modułami; pola, kolejność kodowania, ograniczenia i publiczne eksporty zachowano. Limit maksymalnej ramki serwera to 888 bajtów przy 16 graczach i 16 usunięciach. Wszystkie nowe pliki mają poniżej 500 linii. `fmt --check`, workspace `check` i Clippy offline locked przeszły. Usunięto ostrzeżenie złożonego typu aliasem bez alokacji; duży enum pozostaje bez dodatkowej alokacji. Zachowanie w działaniu pozostaje odroczone.

## R04 — przydział

Jeden wykonawca: `crates/core/src/world.rs` i nowe pliki pod `crates/core/src/world/`. Podzielić reguły, geometrię, ruch, indeks i zapytania kolizji, raycast, stan świata, walkę, snapshoty, parser map i OBJ; zachować publiczne eksporty, algorytmy, kolejność scalania lane'ów i fingerprint. Koordynator integruje i przegląda. Odbiór: pliki <=500 linii, fmt/check/Clippy, przegląd różnic; odroczone scenariusze ruchu, kolizji, raycastu, snapshotów i lane'ów 1/2/auto.

Przegląd: rzeczywiste moduły i fasada publiczna skompilowane; niezależnie porównano oryginalne bloki ruchu, kolizji, geometrii, parserów, fingerprintu, raycastu, walki, snapshotów i scalania partii. Po poprawieniu brakującej funkcji indeksu, derive oraz importów wszystkie pliki mają poniżej 500 linii. `fmt --check`, workspace `check` i Clippy offline locked przeszły. Scenariuszy nie uruchomiono.

## R05 — przydział

Koordynator: `apps/server/src/main.rs`, `apps/server/src/network.rs`, nowy `apps/server/src/lifecycle.rs`, integracja nowych modułów i głównej pętli. Jeden wykonawca: nowe pliki `apps/server/src/storage/` oraz `apps/server/src/storage.rs`; przenieść storage graczy, format zapisu i worker z zachowaniem semantyki, udostępnić niezbędne elementy dla integracji. Drugi wykonawca: nowy `apps/server/src/connection.rs`, stan klienta i kolejki wysyłania; zachować limity i rozliczenie. Oryginały w main usuwa koordynator po przekazaniu kontraktów. Rozdzielić odpowiedzialności serwera i zachować jednego właściciela świata; wysyłanie podlega rotującym budżetom 16 KiB na klienta i 256 KiB na obieg, z zachowaniem częściowych zapisów, kolejek i baseline'u. Odbiór: statyczny przegląd wszystkich ścieżek zamknięcia i limitów, fmt/check/Clippy; budżety są wstępne, pomiary i testy odroczone.

Dodatkowy przydział po zwolnieniu plików: nowy `apps/server/src/replication.rs`, jeden wykonawca; wyodrębnić przygotowanie i kolejkowanie snapshotów z zachowaniem kolejności i aktualizacji baseline'u wyłącznie po przyjęciu ramki. Koordynator włącza moduł w pętlę.

Dodatkowy przydział: nowy `apps/server/src/receive.rs`, jeden wykonawca; wyodrębnić ograniczony odczyt i dekodowanie ramek każdego klienta do funkcji operującej na istniejących buforach i licznikach. Bez zmiany kolejności wiadomości, obsługi EOF i limitu 32; koordynator włącza moduł w pętlę.

Dodatkowy przydział: nowy `apps/server/src/message_dispatch.rs`, jeden wykonawca; przenieść dispatch wiadomości klienta i typ `WorldInput`, zachować kolejność, limity, efekty oraz liczniki; współdzielone odwołania zebrać w kontekst. Koordynator włącza moduł w pętlę.

Dodatkowy przydział: nowy `apps/server/src/world_inputs.rs`, jeden wykonawca; przenieść zastosowanie kolejki wejść świata, zachowując kolejność operacji, rozliczanie czasu, obsługę odejść i komunikatów. Koordynator włącza moduł w fazę ticka.

Dodatkowy przydział: nowy `apps/server/src/send.rs`, jeden wykonawca; przenieść rotacyjny obieg wysyłania z budżetami, rozliczeniem ramek i terminami zamykania. Koordynator włącza moduł w pętlę.

Dodatkowy przydział: nowy `apps/server/src/accept.rs`, jeden wykonawca; przenieść limitowane przyjmowanie połączeń z zachowaniem odmów, inicjalizacji i błędów listenera. Koordynator włącza wynik do ścieżki zamknięcia.

Dodatkowy przydział: nowy `apps/server/src/startup.rs`, jeden wykonawca; wydzielić rozwiązywanie adresu, ładowanie mapy i uruchomienie storage/listenerów z zachowaniem kolejności operacji i czasu życia blokady oraz endpoint guard. Koordynator włącza wynik w główną pętlę.

Przegląd: porównano przeniesiony storage, format v1/v2/v3, odbiór, dispatch, wysyłanie, replikację, start i zamknięcie. Zachowano kolejność faz, rozliczanie częściowych zapisów oraz baseline po zakolejkowaniu; globalny budżet i rotacja zostały przejrzane statycznie. Guard endpointu zwalniany jest przed blokadą danych. Przy kontrolowanym zamknięciu kolejka admin jest opróżniana w limicie, sockety gry zamykane przed zapisem końcowym; błąd wczesnego startu nie tworzy statusu shutdown, jak wcześniej. Pliki serwera poniżej 500 linii. `fmt --check`, workspace `check` i Clippy offline locked przeszły. Testów i pomiarów nie uruchomiono.

## R06 — kontrakt

Koordynator integruje `apps/client/src/main.rs`, `apps/client/src/renderer.rs`, `apps/client/src/bootstrap.rs` i nową wspólną sesję. Sesja odpowiada za socket, ograniczone bufory, handshake, heartbeat i typowane zdarzenia dla obu adapterów. DNS i próby kolejnych adresów mają wspólny deadline poza wątkiem UI; maksymalnie jedna operacja aktywna, identyfikator próby odrzuca spóźnione wyniki, timeout nie tworzy kolejnych porzuconych resolverów. Zachować czyszczenie i własność lokalnego procesu. Przed przydziałem każdej części zapisać zamkniętą listę plików. Odbiór: jeden handshake i heartbeat, fmt/check/Clippy, statyczny przegląd lifecycle; DNS, timeout, reconnect i własność procesu pozostają odroczonymi scenariuszami.

Przydział: nowy `apps/client/src/connector.rs`, jeden wykonawca. Asynchroniczny connector poza UI: jedna aktywna operacja, monotoniczny identyfikator, wspólny deadline DNS i kolejnych adresów, odrzucenie spóźnionego wyniku oraz zakaz rozpoczęcia następnego resolvera do zakończenia poprzedniego. Bez zmian w adapterach; integruje koordynator.

Przydział: nowy `apps/client/src/session.rs`, jeden wykonawca. Wspólna sesja przyjmuje gotowy socket i profil, obsługuje nieblokujące I/O, ograniczone bufory, Identify/Welcome z fingerprintem, heartbeat oraz typowane zdarzenia. Bufor zdarzeń jest przekazywany przez adapter do ponownego użycia. Proces lokalny pozostaje własnością adaptera. Bez zmian adapterów; koordynator integruje i przegląda.

Dalszy przydział do `apps/client/src/connector.rs`: ten sam pojedynczy worker obsługuje również start lub podłączenie lokalnego serwera poza UI i zwraca jego guard; wynik porzuconej próby sprząta własny proces. Żaden drugi worker nie może rozpocząć pracy przed zakończeniem poprzedniej próby.

Dodatkowy przydział: nowy `apps/client/src/terminal_client.rs`, jeden wykonawca; adapter terminala korzysta ze wspólnej sesji i connectora, zachowuje komendy i wyświetlanie zdarzeń oraz własność guardu lokalnego serwera. Koordynator włącza adapter i usuwa dotychczasowe duplikaty z głównego modułu.

W toku: sesja i connector powstały, terminal został do nich podłączony. Connector zachowuje tylko jedną próbę, w tym po timeout; przy wyjściu oczekuje na sprzątanie lokalnego workera. Bufor `state` przechowuje ostatni snapshot, a heartbeat odlicza timeout po zapisie ramki. Workspace `check` przechodzi; pozostają integracja desktopu, przegląd anulowania lokalnego procesu i odbiór R06. Ostrzeżenie `active_attempt` jest tymczasowe podczas integracji.

Dodatkowy przydział: `apps/client/src/renderer.rs`, jeden wykonawca, wyłącznie struktura `Renderer` i jej metody. Zastąpić duplikaty TCP, handshake i heartbeat wspólnymi `ClientSession`/`Connector`, zachować prezentację zdarzeń, snapshoty, predykcję, liczniki i komendy. Start/koniec aplikacji oraz pętlę okna integruje koordynator po zwolnieniu pliku.

Dodatkowy przydział: `apps/client/src/connector.rs`, jeden wykonawca. Usunąć możliwość blokującego sprzątania porzuconego lokalnego guardu w `poll()` przy wyścigu anulowania z gotowym wynikiem; zachować pojedynczą aktywną operację i oczekiwanie na sprzątanie przy zamknięciu.

Przegląd: desktop i terminal używają jednej sesji, a DNS, kolejne adresy i lokalny bootstrap działają w pojedynczym workerze poza UI; timeout i anulowanie nie zwalniają workera przed zakończeniem sprzątania. Odrzucony lokalny guard wraca do workera, a wyjście czeka na jego zakończenie. Wspólny deadline jest przekazywany także do bootstrap. Przejrzano Welcome, heartbeat, zerowy i częściowy zapis, EOF, ponowne połączenie, reset predykcji i metryki. `fmt --check`, workspace `check` i Clippy offline locked przeszły. Pozostaje ostrzeżenie dużego `SessionEvent`: koperta zawiera istniejący duży `ServerMessage`; nie dodano alokacji na wiadomość wyłącznie dla ostrzeżenia. Weryfikacja działania pozostaje odroczona.

## R07 — kontrakt

Koordynator integruje adaptery terminala i F1. Wspólne `ClientCommand`, parser, katalog i wykonanie stanowią jedno źródło pomocy, aliasów i autouzupełniania; wynik jest typowany i prezentowany przez adapter. Bez obiektów alokowanych na każdą komendę, bez formatowania i ponownego parsowania ruchu. Zachować komendy obu interfejsów, jawnie zgłaszać niedostępne możliwości. Istniejący `ServerCommand` i jeden dispatcher serwera pozostają. Odbiór: przegląd zgodności komend i pomocy, fmt/check/Clippy; porównanie działania interfejsów pozostaje odroczone.

Przydział: nowy `apps/client/src/client_commands.rs`, jeden wykonawca. Dostarczyć enum, pojedynczy parser, katalog pomocy i uzupełniania oraz wspólne wykonanie komend protokołu z typowanym wynikiem i żądaniami funkcji adaptera; objąć dotychczasowy zestaw obu interfejsów, w tym host, move, jump, fire i alias exit. Koordynator integruje adaptery i usuwa stare parsery.

Dodatkowy przydział: `apps/client/src/terminal_client.rs` i `apps/client/src/console.rs`, jeden wykonawca. Terminal używa wspólnego parsera i wykonania, a uzupełnianie F1 wyłącznie wspólnego katalogu; usunąć dawny parser z console.rs. Zachować własność procesu i wyświetlanie. Koordynator integruje renderer i moduł.

Przegląd: oba adaptery korzystają z jednego parsera, enumu i wykonania, a pomoc oraz uzupełnianie F1 z katalogu; terminal zgłasza brak renderera przy `fps show`. Alias `exit` i komendy obu interfejsów zachowano. Terminal pozostaje wejściem liniowym bez osobnego mechanizmu Tab. Serwer zachowuje jeden dispatcher. `fmt --check`, workspace `check` i Clippy offline locked przeszły. Porównanie zachowania interfejsów pozostaje odroczone.

## R08 — przydziały

- Renderer: jeden wykonawca, `apps/client/src/renderer.rs`, nowe pliki pod `apps/client/src/renderer/` i pliki shaderów `.wgsl` w tym katalogu. Rozdzielić cykl życia okna, wejście, kamerę, predykcję, zasoby/pipeline'y GPU, geometrię i renderowanie; zachować zachowanie i buforowanie. Sesja zarządza TCP, parser jest wspólny. Koordynator integruje i przegląda.
- Konsola: jeden wykonawca, `apps/client/src/console.rs` i nowe pliki pod `apps/client/src/console/`. Oddzielić stan, historię, HUD, tekst, atlas i motyw; użyć małych struktur kontekstu zamiast długich list argumentów, bez alokacji w klatce. Odbiór obu części: pliki <=500 linii, zachowanie rzeczywistych draw calls, fmt/check/Clippy; działanie okna i GPU odroczone.

Przegląd: moduły i WGSL porównano z poprzednimi shaderami i kodem zasobów, renderowania, predykcji i wejścia; zachowano rysowanie, bufory i rozliczanie draw calls. Parser znajduje się w kontrolerze, a TCP we wspólnej sesji. Konsola ponownie używa również bufora tekstu wejściowego; argumenty rysowania mają konteksty. Nowe pliki poniżej 500 linii; `fmt --check`, workspace `check` i Clippy offline locked przeszły. Testów okna/GPU nie wykonano.

## R09 — przydziały

- Glify: jeden wykonawca, `apps/client/src/console/atlas.rs`, `text.rs`, `theme.rs`, `state.rs` oraz `apps/client/src/renderer/gpu.rs`. Dodać polskie litery do atlasu, jawny glif zastępczy i zachować inne znaki w tekście bez zamiany na `?`; zaktualizować rozmiar tekstury i upload, bez alokacji na klatkę. Koordynator integruje.
- Komunikaty: koordynator ustali wspólny katalog angielskich tekstów z identyfikatorami i typowanymi parametrami oraz podłączy terminal i desktop. Nazwy komend, protokołu i klucze raportu niezależne od języka. Odbiór: wspólne źródło tekstów, fmt/check/Clippy, odroczona weryfikacja glifów i komunikatów.

Dodatkowy przydział komunikatów: jeden wykonawca, `apps/client/src/messages.rs`, `apps/client/src/terminal_client.rs`, `apps/client/src/renderer/session.rs`, `apps/client/src/renderer/controller.rs`, `apps/client/src/main.rs`. Podłączyć wspólny katalog do komunikatów obu interfejsów i uzupełnić typowane parametry; zachować użyteczne szczegóły błędów i działanie. Nie zmieniać ścieżek protokołu ani kluczy raportu.

Dalszy przydział do tych plików: scalić mapowanie błędów kolejki i przyczyn rozłączenia we wspólnym katalogu, zachować szczegół błędu protokołu oraz ujednolicić komunikat o niedostępnym FPS; objąć pozostałe teksty adapterów typowanymi identyfikatorami.

Dalszy przydział: te same pliki oraz `apps/client/src/renderer/window.rs`, `apps/client/src/renderer/camera.rs`; ujednolicić pozostałe teksty użytkownika, w tym ładowanie mapy, błędy okna i ruchu, tytuł i diagnostykę terminala. Nazwy zasobów GPU nie są komunikatami użytkownika. Zachować interfejsy i unikać dodatkowych alokacji na klatkę.

Przegląd: oba adaptery korzystają z katalogu angielskich tekstów oraz wspólnych konwersji typowanych błędów; parser zwraca typ błędu, a opis powstaje w katalogu. Atlas ma dziewięć polskich liter z mapowaniem małych na wielkie, osobną pustą spację i jawny znak zastępczy; rozmiar i rozliczenie GPU są zgodne. Poprawiono brakujące akcenty wykryte statycznie. `fmt --check`, workspace `check` i Clippy offline locked przeszły; wizualne sprawdzenie glifów pozostaje odroczone.

## R10 — kontrakt

Wspólny histogram ma osobne, nazwane przedziały ticków, klatek i RTT; przedziały klatek rozróżniają okolice 8, 16, 20 i 33 ms. Udokumentować jednostki, źródła i przybliżenie percentyli oraz jawne braki próbek; przejrzeć alokacje i propagację faz executora, instrumentacja domyślnie wyłączona. Koordynator integruje i kompiluje oba warianty serwera; przed przydziałem zapisać wyłączną listę plików.

Przydział: jeden wykonawca, `crates/telemetry/src/duration.rs`, `apps/client/src/stats.rs`, `apps/server/src/metrics.rs`, `apps/loadtest/src/main.rs`, `docs/performance.md`, `docs/scenarios/report-v2.md`. Użyć wspólnego mechanizmu histogramów z nazwanymi zbiorami przedziałów, jawnie sygnalizować brak próbki i zachować pola raportu; udokumentować jednostki/źródła. Koordynator przegląda alokacje i fazy, integruje, wykonuje kontrole obu wariantów.

Koordynator integruje również `apps/loadtest/Cargo.toml`, `Cargo.lock` i fazę przygotowania w `apps/server/src/storage/player_store.rs`.

Przegląd: histogramy ticków, klatek i RTT mają osobne przedziały, brak próbek jest jawny, granice porównują pełną wartość czasu. Opisano źródła, jednostki, przybliżenia i zmianę dokładności RTT. Fazy propagują się do zadań executora; przygotowanie snapshotu zapisu przypisano do storage. Udokumentowano istniejące alokacje dispatchu i okresowego próbkowania procesu. Instrumentacja pozostaje domyślnie wyłączona. `fmt --check`, workspace `check`, Clippy i `check` serwera z `allocation-metrics` offline locked przeszły. Pomiary pozostają odroczone.

## R11 — przydziały

- Generator: jeden wykonawca, `apps/loadtest/src/main.rs`, `apps/loadtest/src/config.rs` i nowe moduły w `apps/loadtest/src/` z wyłączeniem `report.rs`. Rozdzielić scenariusze, przygotowanie klientów, workery i statystyki; zachować scenariusze, limity, manifesty i enumy konfiguracji. Koordynator integruje.
- Raport: jeden wykonawca, `apps/loadtest/src/report.rs` i nowe moduły pod `apps/loadtest/src/report/`. Oddzielić dane od serializacji; zachować poprawny JSON, pola v2, dopasowanie endpointu, rozdział konfiguracji i brak tokenów. Odbiór: statyczny przegląd, fmt/check/Clippy; generator nie jest uruchamiany.

Przegląd: porównano scenariusze, przygotowanie, synchronizację i rozliczanie z poprzednią implementacją. Serializacja zachowuje pola v2, escaping JSON, brakujące wartości i kontrolę adresu oraz tożsamości endpointu; token pozostaje prywatny. Format jest przekazywany jako enum. Pliki mieszczą się w limicie. `fmt --check`, workspace `check` i Clippy offline locked przeszły. Generatora nie uruchomiono.

## R12 — przydziały

- Kontrola statyczna: jeden wykonawca, nowy `scripts/check_file_lengths.py` oraz `docs/plan/file-length-policy.md`. Sprawdzać ręcznie utrzymywany kod i shadery pod kątem 500 fizycznych linii, wykrywać nieudokumentowane wyjątki; jawno rozróżnić pliki generowane i zasoby danych. Bez sztucznego łączenia linii i ukrywania kodu.
- Dokumentacja: jeden wykonawca, `README.md`, `docs/architecture.md`, `docs/commands.md`, `docs/performance.md`, `docs/protocol.md`, `docs/scenarios/report-v2.md`. Uzgodnić dokumenty z aktualnym kodem; nie deklarować wyników niewykonanych pomiarów lub testów.
- Koordynator przegląda moduły, własność, `unsafe`, błędy, limity, timeouty, eksporty i ostrzeżenia oraz uruchamia końcowe kontrole w kolejności: fmt, workspace check, Clippy, release build, serwer z allocation-metrics, git diff --check (Cargo locked offline). Wszystkie karty i ustalenia mają mieć dowód; działanie pozostaje odroczone.

Przegląd i dowody: [refactor-review.md](refactor-review.md). Kontrola długości objęła 92 pliki i nie wymaga wyjątków. `fmt --check`, workspace `check`, Clippy, workspace release build, build serwera z `allocation-metrics` i `git diff --check` przeszły. Pozostały dwa ostrzeżenia o rozmiarze enumów oraz środowiskowe ostrzeżenia hardlinków; nie dodano alokacji w gorącej ścieżce. Testów, aplikacji i generatora nie uruchomiono.
