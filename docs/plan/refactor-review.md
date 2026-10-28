# R00–R12: przegląd implementacji

Odebrano 13/13 pakietów implementacyjnych. Dowody szczegółowe, kontrakty i odroczone scenariusze znajdują się w [kartach](cards.md); zakres ustaleń w [macierzy](refactor-matrix.md). Odbiór kodu i kompilacji nie potwierdza grywalności, równoważności lane'ów ani wydajności.

| Pakiet | Dowód przeglądu |
| --- | --- |
| R00 | Macierz, karty, baza ostrzeżeń i zasada 500 linii; zachowano dane użytkownika. |
| R01 | Przejrzano blokadę i publikację profilu, zero-write, limit ramek, konflikty opcji oraz dopasowanie adresu i niezmiennego endpointu próbek raportu. |
| R02 | Typowane konfiguracje i wybory, walidacja startowa, współdzielone nazwy i stałe; adres nasłuchu rozwiązywany przed startem storage. |
| R03 | Porównano eksporty, kodowanie i dekodowanie z v8; limit największej ramki należy do protokołu. |
| R04 | Porównano ruch, kolizje, raycast, fingerprint, parsery, snapshoty i kolejność scalania partii; moduły poniżej limitu. |
| R05 | Przejrzano fazy pętli, rotujące budżety zapisu, częściowy zapis, baseline, storage i ścieżki zamknięcia. |
| R06 | Jeden handshake i heartbeat; pojedynczy worker łączenia, wspólny deadline, odrzucanie spóźnionych wyników i cleanup lokalnego procesu. |
| R07 | Jeden parser, katalog i executor klienta; pomoc i uzupełnianie F1 z katalogu, jeden dispatcher serwera. |
| R08 | Porównano shadery, zasoby GPU, renderowanie, input i predykcję; konsola używa kontekstów oraz ponownie wykorzystywanych buforów. |
| R09 | Wspólny katalog komunikatów, typowane błędy parsera i widoczne polskie glify; spacja i glif zastępczy jawne. |
| R10 | Przedziały tick/frame/RTT i brak próbek opisane; przejrzano alokacje oraz fazy executora, oba warianty serwera skompilowane. |
| R11 | Porównano scenariusze i rozliczenie faz; raport v2 zachowuje pola, escaping i sprawdzanie endpointu bez publikacji tokenu. |
| R12 | Kontrola długości, zgodności dokumentacji, eksportów, błędów, timeoutów, własności i `unsafe`; końcowe kontrole poniżej. |

`unsafe` ogranicza się do delegowania alokacji do `System`, odczytu procesu/plików przez FFI, atomowej podmiany na Windows i systemowego źródła losowości. W przeglądzie sprawdzono przekazywane wskaźniki i rozmiary, czas życia buforów, zamykanie uchwytów oraz przywracanie faz TLS. Liczniki alokacji są opcjonalne, domyślnie wyłączone i nie są licznikiem żywej pamięci. Przegląd statyczny nie zastępuje wykonania na wszystkich platformach.

Kontrole wykonane sekwencyjnie: `cargo fmt --all -- --check`, `cargo check --workspace --locked --offline`, `cargo clippy --workspace --locked --offline`, `cargo build --workspace --release --locked --offline`, `cargo build -p minimium-server --features allocation-metrics --locked --offline`, `git diff --check` oraz `python scripts/check_file_lengths.py`. Kontrola długości objęła 92 pliki; nie ma wyjątków. Clippy nadal wskazuje duże warianty `ServerMessage` i `SessionEvent`; nie dodano pośrednich alokacji wyłącznie w celu usunięcia ostrzeżeń. Środowisko zgłasza także nieudane hardlinki cache incremental i używa kopii.

Żadnych testów, aplikacji ani generatora obciążenia nie uruchomiono. Scenariusze regresji i pomiary opisane w kartach są odroczone do fazy testów wyraźnie rozpoczętej przez użytkownika.
