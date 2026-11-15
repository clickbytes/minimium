# Epic E046 — Dokończenie fundamentów silnika

Status: otwarty. Opisy wykonawcze uporządkowane 2026-11-07; aktualizacja planu nie potwierdza ukończenia implementacji. Źródłem stanu wykonania są dowody w poszczególnych issues i ich rzeczywiste stany GitHub.

## Cel i stan wejściowy

Dokończyć kroki 01–47 przez porównanie wymagań z aktualnym kodem, zachowanie poprawnych rozwiązań i naprawę braków. Punktem wejścia jest całe drzewo robocze D:\Git\minimium, łącznie ze zmianami niezacommitowanymi i plikami untracked. Historyczny punkt review: gałąź codex/epic-008-config-cli-console, HEAD f08e8a9cc385ccf3f97e3abc4622abf0703c0a57. Sam checkout tego SHA nie odtwarza implementacji.

W drzewie istnieją już m.in. rozstrzyganie wejść ruchu, moduły aplikacji, PreparedShadowFrame i znacznik publikacji geometrii. Dawne twierdzenia o ich braku nie są aktualnym audytem. Również wcześniejsze Complete i udane buildy nie potwierdzają końcowego drzewa po późniejszych zmianach. Każde issue zaczyna się od ustalenia rzeczywistej luki, bez powtarzania poprawnych przenosin i bez zamykania zadania samą obecnością typu.

## Jeden wykonawczy tok od 01 do 18

1. Odczytaj AGENTS.md, RTK.md, architecture.toml i README właściciela; zapisz bieżący status, gałąź i HEAD. Zachowaj istniejącą pracę oraz concat.txt. Używaj RTK i istniejącego indeksu CodeGraph; nie twórz indeksu.
2. Wybierz pierwsze niezamknięte issue w kolejności poniżej. Nie zaczynaj następnego przed zamknięciem poprzedniego. Nie realizuj dwóch issues jednocześnie.
3. Przeczytaj pełną instrukcję issue. Zmapuj każde wymaganie na obecnego właściciela, producenta i konsumenta. Spełnione wymaganie udokumentuj; brakujące zaimplementuj wraz ze wszystkimi callsite’ami i usunięciem starej ścieżki.
4. Wykonaj przegląd źródłowy, diff i aktualizację dokumentacji. Raport zawiera wymaganie → dowód w pliku/symbolu → wynik, zmieniony kod, przegląd, stan kompilacji i osobno runtime. Niewyjaśniony wymagany punkt blokuje zamknięcie.
5. Uzupełnij issue na GitHubie rzeczywistymi dowodami i zamknij po spełnieniu jego kryteriów. Ponownie odczytaj stan closed, zaznacz zadanie w tym epicu i dopiero rozpocznij następne. Repo identity guard obowiązuje przed zapisami; bez commit/push.
6. Jeśli późniejsze issue ujawni błąd wcześniejszego kontraktu, zawieś bieżące, ponownie otwórz issue właściciela, popraw i zamknij je z nowymi dowodami. Następnie wróć do zawieszonego. Nie zostawiaj równoległej starej ścieżki i nie prowadź dwóch implementacji jednocześnie.

## Bramki weryfikacji

- E046-01–16: implementacja i przegląd źródłowy. Nie uruchamiaj testów, gry, loadtestów, benchmarków, kompilacji, Clippy, formatowania, skryptów statycznych ani walidatora WGSL. Brak kompilacji przed 17 jest zgodny z planem i nie blokuje źródłowego zamknięcia wcześniejszego issue.
- E046-17: po zamknięciu 01–16 minimalna aktualizacja lockfile, formatowanie oraz dokładnie kontrole i kompilacje z #364. Zapisz prawdziwe wyniki końcowego drzewa. Naprawy wracają do właścicieli pojedynczo; po zmianach ponów dotknięte kontrole.
- E046-18: dowody dla wszystkich 47 kroków, nowe kontrakty, usunięte ścieżki i oddzielny, niewykonany backlog runtime. Najpierw zamknij 18, następnie ponownie odczytaj stan wszystkich 18 issues. Epic jest ukończony wyłącznie gdy wszystkie są closed i raport nie ma nierozstrzygniętego wymagania.

Testy i scenariusze uruchomieniowe wymagają osobnego, jawnego rozpoczęcia przez użytkownika. Kompilacja nie dowodzi wydajności, odporności ani poprawnego wyglądu cieni. Późniejsze dowody integracji nie tworzą zależności zwrotnej blokującej wcześniejsze źródłowe zamknięcie.

## Zakres wspólny

Zachowaj ruch, walkę, czat, portale, światy proceduralne, Earth, dungeon, światy autorskie i oba frontendy. Wersje docelowe: protokół 18, zapis 10, generator 5. Bez migracji, legacy fallbacków, atrap, pustych crate ani szerokich allow(unused/dead_code). Kod i shadery mają limit 500 fizycznych linii; wyjątek wymaga konkretnego powodu i zakresu. Instrukcje szczegółowe i limity są w issues; poniższa tabela ich nie zastępuje.

## Kolejność, pokrycie i kryterium wyjścia

| Issue | Kroki planu | Wymagany rezultat |
|---|---|---|
| [E046-01](https://github.com/clickbytes/minimium/issues/348) | 01–03 | Tabela właściwości i zabezpieczeń wszystkich dzielników shadera |
| [E046-02](https://github.com/clickbytes/minimium/issues/349) | 04–08 | Tabela ścieżek, logowania, limitów i przyczyn błędów |
| [E046-03](https://github.com/clickbytes/minimium/issues/350) | 09–10 | Pełny przepływ manifestu i zweryfikowanej treści do instalacji |
| [E046-04](https://github.com/clickbytes/minimium/issues/351) | 11–16 | Tabela limit/anulowanie/błąd/zwolnienie każdego lifecycle’u |
| [E046-05](https://github.com/clickbytes/minimium/issues/352) | 17–20 | Macierz wszystkich mutujących wiadomości i uprawnień |
| [E046-06](https://github.com/clickbytes/minimium/issues/353) | 21–22 | Ręczne ślady ack, Busy, zastąpienia, braku pakietu i transferu |
| [E046-07](https://github.com/clickbytes/minimium/issues/354) | 23–26 | Spawn/update/remove/despawn przez save/load, limity i inicjalizacja |
| [E046-08](https://github.com/clickbytes/minimium/issues/355) | 27–29 | Tick, Startup, deterministyczny porządek, atomowe efekty i portal |
| [E046-09](https://github.com/clickbytes/minimium/issues/356) | 30–32 | Dwa niezależne ack, ograniczony journal i atomowy full/delta |
| [E046-10](https://github.com/clickbytes/minimium/issues/357) | 33 | Wspólny deadline shutdown i rzeczywisty wynik final save |
| [E046-11](https://github.com/clickbytes/minimium/issues/358) | 34–37 | Macierze PL/EN, fontu, DPI, grafemów, IME i akcji |
| [E046-12](https://github.com/clickbytes/minimium/issues/359) | 38–39 | Własność aplikacji, effective/reason i kontrolowane przejścia bez GPU |
| [E046-13](https://github.com/clickbytes/minimium/issues/360) | 40 | Granica CPU/GPU i publikacja tylko kompletnego zestawu |
| [E046-14](https://github.com/clickbytes/minimium/issues/361) | 41 | Wspólny znacznik caster/receiver i ślady invalidation |
| [E046-15](https://github.com/clickbytes/minimium/issues/362) | 42–44 | Usunięte stare ścieżki i źródłowy przegląd reguł workspace |
| [E046-16](https://github.com/clickbytes/minimium/issues/363) | 45 | Dokumentacja i tabela dowodów kroków 01–45 |
| [E046-17](https://github.com/clickbytes/minimium/issues/364) | 46 | Rzeczywiste wyniki wszystkich wymaganych kontroli końcowego drzewa |
| [E046-18](https://github.com/clickbytes/minimium/issues/365) | 47 | Raport 01–47, backlog runtime, kontrola zamknięcia całego epicu |

## Stan zadań GitHub

W chwili porządkowania wszystkie zadania są otwarte. Checkboxy zmienia wykonawca po ponownym odczycie closed, nie na podstawie historycznego raportu.

- [ ] [[E046-01] Uzgodnić stan początkowy i potwierdzić naprawy numeryczne](https://github.com/clickbytes/minimium/issues/348)
- [ ] [[E046-02] Domknąć audyt ścieżek, diagnostyki i modelu błędów](https://github.com/clickbytes/minimium/issues/349)
- [ ] [[E046-03] Potwierdzić jeden kontrakt światów i resolvera](https://github.com/clickbytes/minimium/issues/350)
- [ ] [[E046-04] Udowodnić ograniczony cykl pracy sektorów, kolizji, cache i HTTP](https://github.com/clickbytes/minimium/issues/351)
- [ ] [[E046-05] Zweryfikować autoryzację każdego wejścia mutującego](https://github.com/clickbytes/minimium/issues/352)
- [ ] [[E046-06] Naprawić semantykę ack i zgodność historii predykcji](https://github.com/clickbytes/minimium/issues/353)
- [ ] [[E046-07] Domknąć trwałe encje i rzeczywistą rezerwację zapisu](https://github.com/clickbytes/minimium/issues/354)
- [ ] [[E046-08] Uzupełnić kontrakt systemów gameplay i lifecycle świata](https://github.com/clickbytes/minimium/issues/355)
- [ ] [[E046-09] Dokończyć audyt transakcji i ograniczeń replikacji](https://github.com/clickbytes/minimium/issues/356)
- [ ] [[E046-10] Potwierdzić shutdown z rzeczywistym wynikiem zapisu](https://github.com/clickbytes/minimium/issues/357)
- [ ] [[E046-11] Zamknąć pokrycie PL/EN, fontu i wejścia](https://github.com/clickbytes/minimium/issues/358)
- [ ] [[E046-12] Dokończyć SettingsStore i rzeczywiste wydzielenie aplikacji](https://github.com/clickbytes/minimium/issues/359)
- [ ] [[E046-13] Zakończyć rozdział przygotowania CPU i publikacji GPU](https://github.com/clickbytes/minimium/issues/360)
- [ ] [[E046-14] Powiązać castery i odbiorców z tą samą publikacją geometrii](https://github.com/clickbytes/minimium/issues/361)
- [ ] [[E046-15] Usunąć stare ścieżki i naprawić kontrolę architektury](https://github.com/clickbytes/minimium/issues/362)
- [ ] [[E046-16] Uzgodnić dokumentację ze stanem kodu](https://github.com/clickbytes/minimium/issues/363)
- [ ] [[E046-17] Wykonać jedyną końcową fazę kompilacji i walidacji](https://github.com/clickbytes/minimium/issues/364)
- [ ] [[E046-18] Zamknąć epic raportem i oddzielnym backlogiem uruchomieniowym](https://github.com/clickbytes/minimium/issues/365)
