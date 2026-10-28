# Macierz kryteriów Epic 001

Odebrana część implementacyjna: **67/67** pozycji (61 pól checklist #1–#12 oraz 6 jawnych kontraktów pierwszego etapu z #1). Nie oznacza to potwierdzenia sformułowań „działa” ani wyniku pomiaru. **Potwierdzone scenariusze działania: 0** — wykonanie odroczone.

#6 i #11 pozostają `closed/not_planned`, bez zmiany znaczenia ich zamknięcia. Testy i pomiary wspomniane w kryteriach mają gotowy kod/procedurę, lecz nie są wykonane. Dowody: [przegląd](review.md), [kompilacja](verification.md), [przyszła weryfikacja](../prototype.md).

| ID | Wymaganie źródłowe | Konkretny symbol / dokument | Pakiet | Odbiór implementacji |
|---|---|---|---|---|
| 1.1 | Opis projektu i zasady powyżej są zapisane w dokumentacji repozytorium. | docs/vision.md; docs/adr/ | K00 D01 D02 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 1.2 | Powstaje Rust workspace z oddzielnymi programami klienta, serwera i `minimiumctl`. | Cargo.toml; apps/*/Cargo.toml | K00 D01 D02 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 1.3 | Udokumentowane są granice modułów, przepływ danych i model własności stanu świata. | docs/architecture.md: Current module contracts | K00 D01 D02 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 1.4 | Istnieje konfiguracja liczby wątków oraz poprawny tryb jednordzeniowy. | Executor::new/map; server::run | K00 D01 D02 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 1.5 | Działa konsola klienta i wspólny rejestr komend administracyjnych serwera. | Console::build_vertices; commands::execute | K00 D01 D02 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 1.6 | Serwer przyjmuje `status` i `stop` przez lokalny kanał administracyjny bez `stdin`. | AdminServer::poll; admin_client::request/stop_and_wait | K00 D01 D02 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 1.7 | Działa pierwszy powtarzalny test obciążenia oraz raport CPU, pamięci i sieci. | run_worker; report::print_report; RuntimeMetrics | K00 D01 D02 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 1.8 | Prototyp dwóch klientów i serwera demonstruje pełne 3D oraz tę samą symulację w trybie lokalnym i sieciowym. | World::step_players_parallel; map_geometry; LocalServer | K00 D01 D02 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 2.1 | Dokumenty wizji, architektury, protokołu i wydajności są spójne z Epic 001. | docs/architecture.md; docs/protocol.md; docs/performance.md | D01 D02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 2.2 | Każda ważna decyzja ma uzasadnienie i zapis ADR. | docs/architecture.md; docs/protocol.md; docs/performance.md | D01 D02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 2.3 | Cel 500 graczy na 500 MHz ma jawne warunki scenariusza; nie jest deklaracją aktualnej wydajności. | docs/architecture.md; docs/protocol.md; docs/performance.md | D01 D02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 2.4 | Dokumentacja rozróżnia stan obecny od architektury docelowej. | docs/architecture.md; docs/protocol.md; docs/performance.md | D01 D02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 3.1 | Wszystkie programy budują się osobno i razem. | Cargo.toml; apps/*/Cargo.toml; run | K00 I01 D02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 3.2 | Granice crate'ów odpowiadają rzeczywistym odpowiedzialnościom; brak pustych crate'ów. | Cargo.toml; apps/*/Cargo.toml; run | K00 I01 D02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 3.3 | Serwer nie zależy od GPU, UI ani audio. | apps/server/Cargo.toml; cargo tree (core/protocol/telemetry) | K00 I01 D02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 3.4 | README podaje uruchomienie klienta, serwera i minimiumctl. | Cargo.toml; apps/*/Cargo.toml; run | K00 I01 D02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 4.1 | Modyfikacje świata odbywają się na granicy faz symulacji. | Executor::map; World::step_players_parallel; run; RuntimeMetrics | S01 S02 T01 T02 T03 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 4.2 | Wątki sieciowe i administracyjne nie modyfikują świata bezpośrednio. | Executor::map; World::step_players_parallel; run; RuntimeMetrics | S01 S02 T01 T02 T03 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 4.3 | Tryby 1 i 2 dają ten sam stan autorytatywny dla tego samego wejścia. | Executor::map; World::step_players_parallel ordered merge | S01 S02 T01 T02 T03 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 4.4 | Kolejki są ograniczone, proces nie czeka aktywnie podczas bezczynności. | Executor::map; World::step_players_parallel; run; RuntimeMetrics | S01 S02 T01 T02 T03 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 4.5 | Metryki pokazują liczbę wątków i czas faz ticku. | Executor::map; World::step_players_parallel; run; RuntimeMetrics | S01 S02 T01 T02 T03 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 5.1 | Format protokołu jest niezależny od konkretnego transportu. | decode_client/decode_server; queue_message; diff_remote_players | S03 T03 C03 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 5.2 | Nieprawidłowe i zbyt duże wiadomości mają kontrolowaną obsługę. | decode_client/decode_server; queue_message; diff_remote_players | S03 T03 C03 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 5.3 | Serwer wysyła klientowi tylko istotne obiekty. | WorldSnapshot::visit_nearby; diff_remote_players | S03 T03 C03 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 5.4 | Mierzone są bajty, pakiety i koszt kodowania na gracza. | queue_message; account_output; network::flush; client_traffic | S03 T03 C03 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 5.5 | Dwa klienty mogą otrzymać poprawny stan z jednego serwera. | decode_client/decode_server; queue_message; diff_remote_players | S03 T03 C03 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 6.1 | Konsola działa w oknie klienta. | Console::build_vertices; App::window_event | C01 C03 C04 D02 (issue not_planned) | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 6.2 | `help`, `connect`, `disconnect`, `fps show`, `net stats` i `quit` działają. | parse_command; Renderer::execute_console_input | C01 C03 C04 D02 (issue not_planned) | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 6.3 | Historia i podpowiedzi są dostępne. | Console::{history_up,history_down,complete_input} | C01 C03 C04 D02 (issue not_planned) | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 6.4 | Komendy klienta mają wspólny schemat z dokumentacją i testem parsera. | ClientCommand; parse_command; docs/commands.md | C01 C03 C04 D02 (issue not_planned) | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 7.1 | `status`, `players list`, `metrics show`, `world save`, `stop` działają przez terminal i minimiumctl. | ServerCommand::parse; run; PersistenceWorker; ServerEndpoint | B01 B02 S01 S02 C02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 7.2 | `status` i `stop` działają przy stdin zamkniętym. | ServerCommand::parse; run; PersistenceWorker; ServerEndpoint | B01 B02 S01 S02 C02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 7.3 | Komendy zmieniające świat trafiają do kolejki symulacji. | ServerCommand::parse; run; PersistenceWorker; ServerEndpoint | B01 B02 S01 S02 C02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 7.4 | Ponowny start i zamknięcie porządkują endpoint oraz stan procesu. | ServerCommand::parse; run; PersistenceWorker; ServerEndpoint | B01 B02 S01 S02 C02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 7.5 | Logi administracyjne odnotowują zmiany stanu. | ServerCommand::parse; run; PersistenceWorker; ServerEndpoint | B01 B02 S01 S02 C02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 8.1 | Serwer ładuje mapę i odrzuca niepoprawne dane. | WorldMap::parse/load_file; CharacterInput; World::step_players_parallel; CollisionQueryStats | W01 T03 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 8.2 | Gracz porusza się po schodach i pod mostem bez przenikania przez geometrię. | demo.map AABB; step_character; MAX_STEP_UP | W01 T03 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 8.3 | Kolizje są autorytatywne i niezależne od renderera. | WorldMap::parse/load_file; CharacterInput; World::step_players_parallel; CollisionQueryStats | W01 T03 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 8.4 | Statyczne i dynamiczne obiekty mają osobne dane kolizji. | WorldMap::collision_index; CharacterState; World::fire_hitscan | W01 T03 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 8.5 | Zapytania przestrzenne są mierzone przy obciążeniu. | WorldMap::parse/load_file; CharacterInput; World::step_players_parallel; CollisionQueryStats | W01 T03 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 9.1 | Klient wyświetla scenę kontrolną z dwoma piętrami i mostem. | Renderer::render; App; RemotePlayerTrack | C01 C02 C03 C04 W01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 9.2 | Mysz i klawiatura sterują kamerą i ruchem FPP. | Renderer::render; App; RemotePlayerTrack | C01 C02 C03 C04 W01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 9.3 | Drugi klient widzi pierwszego w odpowiedniej pozycji. | Renderer::render; App; RemotePlayerTrack | C01 C02 C03 C04 W01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 9.4 | `fps show` i statystyki renderowania pokazują czas klatki i liczbę operacji rysowania. | FrameStats::record_frame/summary; Renderer::render/resize | C01 C02 C03 C04 W01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 9.5 | Klient działa przy serwerze lokalnym oraz osobno uruchomionym. | Renderer::render; App; RemotePlayerTrack | C01 C02 C03 C04 W01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 10.1 | `world save` tworzy kompletny, wersjonowany zapis dla sceny prototypowej. | PlayerStore::load/save_snapshot; SaveSnapshot::write; PersistenceWorker | B01 B02 C02 S02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 10.2 | Ponowny start odtwarza zapisany stan. | PlayerStore::load/save_snapshot; SaveSnapshot::write; PersistenceWorker | B01 B02 C02 S02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 10.3 | `stop` kończy pracę po opróżnieniu koniecznych operacji zapisu. | PlayerStore::load/save_snapshot; SaveSnapshot::write; PersistenceWorker | B01 B02 C02 S02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 10.4 | Uszkodzony lub niezgodny zapis daje czytelny błąd. | PlayerStore::load/save_snapshot; SaveSnapshot::write; PersistenceWorker | B01 B02 C02 S02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 10.5 | Długie I/O nie blokuje ticku symulacji. | PlayerStore::load/save_snapshot; SaveSnapshot::write; PersistenceWorker | B01 B02 C02 S02 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 11.1 | Scenariusze 2, 100 i 500 klientów są zapisane z parametrami ruchu i czasem trwania. | RuntimeMetrics; run_worker; RttStats; print_report | T01 T02 T03 C04 R01 R02 (issue not_planned) | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 11.2 | Raport zawiera CPU, pamięć, alokacje, fazy ticku, opóźnienia i bajty na gracza. | RuntimeMetrics; run_worker; RttStats; print_report | T01 T02 T03 C04 R01 R02 (issue not_planned) | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 11.3 | Wyniki zapisują sprzęt, system, profil kompilacji i liczbę wątków. | ReportMetadata; build.rs; ProcessSampler::sample | T01 T02 T03 C04 R01 R02 (issue not_planned) | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 11.4 | Porównanie 1 i 2 wątków jest powtarzalne. | RuntimeMetrics; run_worker; RttStats; print_report | T01 T02 T03 C04 R01 R02 (issue not_planned) | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 11.5 | Raport obejmuje średnią i wysokie percentyle; cel 500 MHz jest oznaczony jako badawczy. | RuntimeMetrics; run_worker; RttStats; print_report | T01 T02 T03 C04 R01 R02 (issue not_planned) | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 12.1 | Jeden serwer i dwa klienty chodzą po mapie pełnego 3D. | run server/client; bootstrap::connect_or_start; WorldMap; PlayerStore | C01 C02 C03 W01 D02 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 12.2 | Gracze widzą siebie, a serwer rozstrzyga ruch i kolizje. | run server/client; bootstrap::connect_or_start; WorldMap; PlayerStore | C01 C02 C03 W01 D02 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 12.3 | `minimiumctl status` i `stop` działają bez terminala serwera. | run server/client; bootstrap::connect_or_start; WorldMap; PlayerStore | C01 C02 C03 W01 D02 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 12.4 | Ten sam scenariusz działa z `--threads 1` i `--threads 2`. | run server/client; bootstrap::connect_or_start; WorldMap; PlayerStore | C01 C02 C03 W01 D02 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 12.5 | Zapis i ponowne uruchomienie odtwarzają ustalony stan. | run server/client; bootstrap::connect_or_start; WorldMap; PlayerStore | C01 C02 C03 W01 D02 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 12.6 | Pomiar zasobów jest dołączony do opisu prototypu. | FrameStats; RuntimeMetrics; report v2; final measurement checklist | C01 C02 C03 W01 D02 I01 | odebrane — [przegląd](review.md), [statyka](verification.md) |
| 1.C1 | Oddzielne client/server/ctl. | Cargo.toml; apps/*/Cargo.toml | I01 | odebrane — [przegląd](review.md) |
| 1.C2 | Tryby1 i2, ten sam scenariusz procesowy. | Executor::map; prototype.md (wykonanie odroczone) | I01 | odebrane — [przegląd](review.md) |
| 1.C3 | status i stop bez terminala. | AdminServer::poll; commands::execute; admin_client::stop_and_wait | I01 | odebrane — [przegląd](review.md) |
| 1.C4 | Pełny tick/fazy, pamięć i sieć raportu. | RuntimeMetrics; ProcessSampler; report::capture_server | I01 | odebrane — [przegląd](review.md) |
| 1.C5 | Piętra, schody, most i przejście pod nim. | demo.map; step_character; docs/maps.md AABB | I01 | odebrane — [przegląd](review.md) |
| 1.C6 | Publiczne interfejsy, wersjonowanie, rozszerzanie gry. | docs/architecture.md: Current module contracts; docs/protocol.md | I01 | odebrane — [przegląd](review.md) |

