# Epic 001 — implementacja

Status: **implementacja odebrana, weryfikacja działania odroczona**. Odebrane: 19/19 pakietów, 67/67 części implementacyjnych kryteriów (61 checklist + 6 kontraktów). Potwierdzone scenariusze działania: 0. Odbiór obejmuje kod, kompilację i przegląd. Weryfikacja działania, testy i obciążenie są odroczone do osobnego polecenia użytkownika.

Zakres zamrożony: protokół v8, zapis v3, obecny model świata, trwałe ID/nazwa/pozycja/zdrowie graczy i fingerprint statycznej mapy. Poza zakresem konta, TLS, ekwipunek, NPC, strefy, edytor, audio, przeglądarka. 500 graczy na 500 MHz jest celem badawczym.

Zespół: koordynator i do trzech gpt-6-luna low. Jeden właściciel pliku; manifesty, eksporty, pętle, unsafe i allocator należą do koordynatora. Shell zawsze przez rtk. Brak commitów/pushów wykonawców. Zachowujemy istniejący diff. Kompilacje tylko sekwencyjnie u koordynatora.

Statusy: gotowe do przydzielenia → w pracy → do przeglądu → odebrane. Tylko koordynator odbiera. Raport obejmuje pliki, każdy warunek, statykę i nierozwiązane kwestie.

- [K00](tasks/K00.md) — Baza i kontrakty; zależności: brak.
- [B01](tasks/B01.md) — Wyłączna własność katalogu serwera; zależności: K00.
- [B02](tasks/B02.md) — Wspólny klient administracyjny; zależności: K00.
- [C01](tasks/C01.md) — Niezależne opcje klienta; zależności: K00.
- [C02](tasks/C02.md) — Własność lokalnego procesu; zależności: B01 B02 C01.
- [C03](tasks/C03.md) — Jawny handshake klienta; zależności: C01.
- [S01](tasks/S01.md) — Nieblokujące admin IPC; zależności: K00.
- [S02](tasks/S02.md) — Wspólny dispatcher i ograniczone komendy; zależności: S01.
- [S03](tasks/S03.md) — Rozliczenie częściowych zapisów gry; zależności: K00.
- [W01](tasks/W01.md) — Ciągła trasa pełnego3D; zależności: K00.
- [T01](tasks/T01.md) — Wspólne duration/process; zależności: K00.
- [T02](tasks/T02.md) — Opcjonalny licznik alokacji; zależności: T01.
- [T03](tasks/T03.md) — Brakujące metryki serwera; zależności: S02 S03 T01 T02.
- [C04](tasks/C04.md) — Rzeczywisty koszt klatki; zależności: T01.
- [R01](tasks/R01.md) — Powtarzalne scenariusze; zależności: K00.
- [R02](tasks/R02.md) — Raport v2; zależności: B02 T03 R01.
- [D01](tasks/D01.md) — Wizja i ADR; zależności: K00.
- [D02](tasks/D02.md) — Instrukcja i kontrakty; zależności: C02 C03 S02 W01 C04 R02.
- [I01](tasks/I01.md) — Integracja i odbiór; zależności: wszystkie.

[Macierz kryteriów](criteria.md), [baza ostrzeżeń](baseline.md). Każdy wiersz macierzy ma oddzielny odbiór implementacji; żaden nie jest dowodem działania.

## Bramka odbioru

1. Wszystkie wymagane kryteria mają symbol i przegląd, moduły są podłączone bez atrap/todo.
2. fmt --check, check --workspace --locked --offline, clippy --workspace --locked --offline, release build workspace, server feature allocation-metrics, git diff --check przechodzą.
3. Brak nowych ostrzeżeń/wyciszeń, v8 i v3 zachowane, wszystkie bufory/queue ograniczone.
4. Dwa profile i wspólny serwer opisane; żadnych wykonanych testów/obciążenia ani deklaracji grywalności.
5. #6 i #11 pozostają closed/not_planned; pakiety C03/C04/T01–T03/R01/R02 opisują brakujące prace, nie zmieniając znaczenia tych issue.

## Odroczona weryfikacja działania

Dwa profile i odrzucenie duplikatu; schody/platforma/most/pod mostem; owner/attached shutdown; błędy okna/renderera bez sierot; slow admin; overflow/token/partial I/O; dwa serwery ten sam katalog; save/restart/recovery/error; full/delta/removal snapshots; równoważna symulacja 1/2/auto z co najmniej 32 graczami; 2/100/500 clustered/dispersed ze sprzętem i konfiguracją. Potwierdzone scenariusze: 0 (odroczone).


Dowody końcowe: [przegląd](review.md) i [statyka/kompilacje](verification.md). Bez commitów i pushów; GitHub Epic aktualizowany jako odbiór implementacji, z otwartą odroczoną weryfikacją.
