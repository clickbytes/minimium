# EPIC 002 — Stabilizacja architektury Minimium

Kanoniczny Epic: https://github.com/clickbytes/minimium/issues/14.
Bazowy main: 53314514dac1456744af909aa4a1ea910458d111 (2026-10-28).
Integracja: epic/002-architecture-stabilization -> main; zadaniowe PR kierujemy WYŁĄCZNIE do gałęzi integracyjnej.

## Cel i architektura

Core = czysty model i reguły świata, typowane ID, collision, physics i domain invariants. Nie zawiera TCP, plików, administracji, wykonawcy, telemetry ani danych przykładowej gry. Protocol = wyłącznie wire v8, typowane wiadomości i codec, ewentualnie jednokierunkowo zależy od core dla typów ID. Platform = operacje FS/OS, endpoint, lock, profil storage, admin/control. Assets = map/OBJ loader/parser i względne ścieżki. Executor, tick, queues i persistence mają właścicieli w serwerze. Client łączy session/connector z oddzielnym gameplay, GPU, UI i input. Telemetry jest niezależne od domeny. Nie tworzymy pustych crate'ów.

## Zadania i odnośniki

| Task | Issue | Zależność |
| --- | --- | --- |
| E002-00 Baseline + PR #13 | [#15](https://github.com/clickbytes/minimium/issues/15) | start |
| E002-01 Typy domenowe | [#16](https://github.com/clickbytes/minimium/issues/16) | 00 |
| E002-02 Protocol v8 | [#17](https://github.com/clickbytes/minimium/issues/17) | 01 |
| E002-03 Platform/admin | [#18](https://github.com/clickbytes/minimium/issues/18) | 01,02 |
| E002-04 Asset/map/OBJ | [#19](https://github.com/clickbytes/minimium/issues/19) | 03 |
| E002-05 Symulacja/executor | [#20](https://github.com/clickbytes/minimium/issues/20) | 01,04 |
| E002-06 ServerRuntime | [#21](https://github.com/clickbytes/minimium/issues/21) | 02,03,05 |
| E002-07 TCP/admission | [#22](https://github.com/clickbytes/minimium/issues/22) | 02,06 |
| E002-08 WorldInput | [#23](https://github.com/clickbytes/minimium/issues/23) | 06,07 |
| E002-09 Storage/shutdown | [#24](https://github.com/clickbytes/minimium/issues/24) | 03,06,08 |
| E002-10 Client session/bootstrap | [#25](https://github.com/clickbytes/minimium/issues/25) | 02,03,06 |
| E002-11 Renderer/UI | [#26](https://github.com/clickbytes/minimium/issues/26) | 04,05,10 |
| E002-12 Config/commands | [#27](https://github.com/clickbytes/minimium/issues/27) | 03,10,11 |
| E002-13 Telemetry/loadtest | [#28](https://github.com/clickbytes/minimium/issues/28) | 05,07,09,11,12 |
| E002-14 Arch audit/docs | [#29](https://github.com/clickbytes/minimium/issues/29) | 01–13 |
| E002-15 Final gate + MERGE | [#30](https://github.com/clickbytes/minimium/issues/30) | all |

## Niezmienniki

- Binary protocol v8: brak zmiany kolejności, długości, oznaczeń, limitów i semantyki ramek.
- World snapshot v3 z migracją v2, profil v1, endpoint, admin token ownership, format shutdown i kanoniczny fingerprint mapy pozostają zgodne. Żadnego resetowania danych.
- Serwer jest autorytatywny, tylko main lane mutuje world, kolejka wejść i bounded outgoing, worker wyniki scalane w kontrolowanym miejscu.
- Owned local process i Attached mają osobne, trwałe znaczenie. Klient nie zatrzymuje cudzego serwera.
- Nie wprowadzać alokacji i locków bez uzasadnienia na tick/frame. Zachować 500 fizycznych linii na plik kodu (AGENTS.md).
- Każdy task musi zakończyć się własnym PR do gałęzi epica i uzupełnieniem issue o pliki, decyzje, komendy statyczne, review, PR/SHA i nierozwiązane problemy.

## Quality gates

Kontrole bez testów: cargo fmt/check/clippy/build, release, allocations feature, git diff --check, scripts/check_file_lengths.py, docelowo scripts/check_architecture.py. Faktyczne wyniki notować w epic-002-verification.md; nieudowodnione pozostaje NOT RUN.

AGENTS.md odracza wszystkie testy i uruchamianie aplikacji/generatora do końcowej fazy wyraźnie rozpoczętej przez użytkownika. E002-15 ma wykonać finalną weryfikację dopiero po tej decyzji. Bez niej końcowy PR pozostaje DRAFT/BLOCKED. Epic #14 zamykamy dopiero po MERGE jednego końcowego PR integracyjnego do main oraz zapisaniu merge SHA.

Wcześniejszy PR #13 (https://github.com/clickbytes/minimium/pull/13) należy do E002-00, bazą ma być epic branch, wymaga compile/static review przed merge.
