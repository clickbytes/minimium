# Odbiór implementacji Epic 001

Koordynator przejrzał integrację po raportach trzech wykonawców gpt-6-luna low. Odebrano 19/19 pakietów i część implementacyjną 67/67 wymagań macierzy. Weryfikacja działania pozostaje odroczona: żadnego testu ani generatora nie uruchomiono. Przegląd nie dowodzi grywalności, równoważności lane’ów ani wydajności.

| Pakiet | Dowód i wynik przeglądu |
|---|---|
| K00 | Rust1.99.0, baza ostrzeżeń, macierz i kontrakty; początkowy diff oraz nieśledzone mapy/OBJ zachowane. |
| B01 | ServerDataLock przed odczytem zapisu i zmianą plików. File::try_lock, bez truncate/usuwania lock. Endpoint: atomowa podmiana, limit256B, redakcja Debug. EndpointGuard sprząta tylko zgodną instancję. |
| B02 | CTL i bootstrap używają admin_client. Limit2s connect,3s całej odpowiedzi,64KiB; stop120s. Sukces wymaga OK, zgodnego owner, braku endpointu i zwolnionej blokady. Powtórny odczyt wyniku pod blokadą. |
| C01 | ClientOptions w obu trybach; profile_dir tylko do load_or_create, server_data_dir do bootstrap. ID ładowany raz, nie przy reconnect. Unknown/missing/duplicate odrzucane; lanes1/2/auto, default1. |
| C02 | Attached nie wysyła stop. OwnedServer ma RAII od spawn, prywatny nonce identyfikuje endpoint. Przegrany start czeka do deadline na zwycięzcę. Własne dziecko kill+wait. Błędy okna/renderera/event_loop przechodzą przez guard. Rehost przejmuje nowy Owned, błędy cleanup są raportowane. |
| C03 | Wspólny ConnectionState; tylko Welcome ze zgodną mapą kończy handshake,5s od Identify. Timeout/refusal/EOF/protocol/mismatch czyszczą bufory. Ruch, strzał i heartbeat czekają na Welcome. ERROR_BUSY nie oznacza wyłącznie profilu. |
| S01 | AdminServer bez World/storage/wątku/blokującego I/O. Max8 połączeń/4 accepts,512B linii,64KiB reply,2s request/reply. Partial/zero-write obsłużone. ID monotoniczne, emit once; dispatcher pomija wygasłe żądania przez is_pending. |
| S02 | Jedna kolejka i commands::execute dla obu adapterów; max8/tick przed symulacją. Terminal i dispatch po64; linia512B. Lista graczy/wyniki identyczne. Stop reply opróżniany przed końcowym storage, oczekujące admin dostają stopping. |
| S03 | flush zwraca bajty również przy błędzie; WouldBlock zachowuje resztę, WriteZero fatalny. Bajty liczone przed odłączeniem, profil zwalniany w tym obiegu. Pozycja utrwalana przed release dla szybkiego reconnect. Baseline dopiero po przyjęciu ramki. |
| W01 | Geometria zgodna z tabelą; AABB schodów/podestu/mostu/platformy stykają się. Pod mostem3.6. Rampa/trójkąty poza trasą, OBJ zachowane. Fingerprint zmieniony jawnie; brak resetu/usunięcia istniejących zapisów. |
| T01 | DurationStats zachowuje histogram. Telemetry niezależne od świata/protokołu/UI. ProcessSampler max1Hz: RSS/private, CPU i rzeczywiste wątki, None/n/a. Windows skompilowany; Linux przejrzany, bez kompilacji krzyżowej i uruchomienia. |
| T02 | Feature domyślnie off, System zachowany. Udane alloc/alloc_zeroed/realloc i bajty żądane; TLS phase dziedziczony przez Executor::map. Storage osobno. Oba warianty skompilowane; unsafe przejrzany poniżej. |
| T03 | Hooki decode/admission/write, encoding, decode→apply, wiek najstarszej ramki. Wyjście max256 ramek/64KiB. Per-client liczniki + connected_ms pozwalają wyliczyć tempo; disconnect loguje końcowe wartości. Jednostki w performance.md. |
| C04 | Rzeczywisty redraw interval/update+render wall work; process CPU osobno. render zwraca faktyczne draw,0 przy pominięciu. GPU=size() buforów+atlas+depth, aktualizacja resize, nie pamięć sterownika. |
| R01 | Stare4 scenariusze zachowane. Clustered naprzemienny ruch; dispersed kąt2π*ID/liczba klientów, preencoded ramka poza hotloop.18 manifestów2/100/500×1/2/auto×2,60s, ustawienia procesów oddzielone; TOML sprawdzony statycznie. |
| R02 | v2 zachowuje stare pola, dodaje run/build/scenario/hardware, konfigurację generatora i rzeczywiste próbki serwera. Admin tylko przed/po workerach. Token nie trafia do raportu. Escaping JSON, null dla braku danych; osobny schemat. |
| D01 | Wizja i3 ADR rozróżniają przyjęte decyzje, kod i przyszłość; 500MHz badawcze. |
| D02 | README/commands/prototype/performance/schema uzgodnione z parserami/metrikami. Dwa profile+wspólny serwer w nowym katalogu. Procedury oddzielone od wykonanych wyników. |
| I01 | Moduły podłączone, bez atrap/todo i dodatkowych testów. Graf serwera tylko core/protocol/telemetry. Kompilacje/statyka w verification.md. #6/#11 pozostają not_planned. |

## Poprawki z przeglądu

Poprawiono raw OK/ERR w bibliotece admin, brak stop we wspólnym API, odczyt wyniku bez limitu, race odczytu wyniku/restartu, brak deadline reply admin, zero-write, wykonywanie wygasłych komend, pomijanie final-save przy błędzie listenera, brak odpowiedzi za stop, osiem skupisk w pierwotnym dispersed, porzucanie Owned przy rehost, brak fire w tekście, nieograniczone stdin klienta i ciche błędy cleanup. Poprawki włączono przed końcową kompilacją.

Prywatny server.shutdown.owner wiąże wynik z pełnym tokenem instancji. Jest publikowany atomowo przed starym server.shutdown i usuwany z nim przy starcie pod lock. Port sam nie jest wystarczającą tożsamością. Format server.shutdown oraz protokół gry i snapshot świata pozostają bez zmian.

## Przegląd unsafe

CountingAllocator deleguje pointer/Layout/size do System i liczy po nie-null. Const TLS Cell i relaxed atomics nie alokują, nie używają mutexów/logów i nie mają ścieżek paniki. Guard przywraca fazę. Snapshot liczników jest skumulowaną obserwacją, nie transakcją zatrzymującą inne wątki; realloc bytes to nowy żądany rozmiar, nie delta/live memory. Bez feature brak TLS i operacji liczników w hot path.

Windows FFI ma repr(C), zainicjalizowany rozmiar i żywe bufory. Pseudohandle procesu jest pożyczony; snapshot procesów zamykany raz. CPU sumuje user+kernel w100ns zgodnie z [GetProcessTimes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes); wątki z [PROCESSENTRY32W](https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/ns-tlhelp32-processentry32w). Linux: /proc/self/stat z obsługą nawiasów/spacji nazwy i sysconf(_SC_CLK_TCK), Threads z /proc/self/status. Nieudany pomiar nie staje się zerem.

Istniejący MoveFileExW przeniesiony do core::files zachowuje replace-existing/write-through; zakończone zerem ścieżki UTF16 żyją przez call. RNG zachowany. [File::try_lock](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock) i RAII zastępują potrzebę unsafe własnej blokady. Lock zwalniany po storage/endpoint cleanup.

Nowe kolejki i bufory mają limity opisane w performance.md. Błąd runtime próbuje final-save i zapisuje ERR. Nagłe przerwanie procesu/zasilania nie jest kontrolowanym zapisem. Odmowa OS dla kill/reap daje jawny błąd, nigdy zapewnienie o usunięciu procesu.

## Odroczone dowody

Pełny wykaz: prototype.md i epic-001.md — dwa profile, trasa, lifecycle, admin slow/overflow/partial, lock race, storage error/restart, full/delta/removal, identyczne tick-inputy1/2/auto przy co najmniej32 graczach i18 pomiarów. Żadnego nie wykonano. Test parsera #6.4 i pomiary #8.5/#11/#12.6 pozostają odroczone; odebrano kod i przygotowanie, nie wyniki.
