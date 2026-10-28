# Epic 002 — ownership i kontrakty

Pełny zakres, implementacja krok po kroku, kryteria odbioru i blokery są w issues #15–#30 pod Epic #14. Ten plik rejestruje wzajemne granice i kolejność, a nie duplikuje całej specyfikacji.

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
