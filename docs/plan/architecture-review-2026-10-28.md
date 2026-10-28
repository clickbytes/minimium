# Przegląd architektury Minimium — 2026-10-28

Zakres: statyczny przegląd `main` na commit `53314514`. Repozytorium ma już `core`, `protocol` i `telemetry`; mechaniczne dzielenie plików nie jest równoznaczne z dobrą architekturą. Przegląd nie jest potwierdzeniem kompilacji, testów, grywalności lub wydajności. `AGENTS.md` odracza testy do wyraźnie rozpoczętej przez użytkownika fazy końcowej.

## Ustalenia

| Priorytet | Granica | Obserwacja i zalecenie |
| --- | --- | --- |
| P1 | Core | `crates/core/src/lib.rs` łączy świat z admin IPC, filesystem, profilem, lock i executorem. `world_state.rs` bezpośrednio używa `Executor`, który używa telemetry. Wyodrębnić czysty model/reguły świata od wykonywania oraz adapterów I/O. |
| P1 | Inwariant indeksu | `World::join_at_with_health` dopuszczał zastąpienie ID bez usunięcia wcześniejszych wpisów komórek. Ten PR usuwa stare członkostwo przez `leave` przed zastąpieniem gracza. |
| P1 | Typy | Publiczne `u64` gracza, `ServerMessage::Error(u8)`, identyfikator profilu raz `PlayerProfileId`, a raz `[u8; 16]`. Docelowo `PlayerId` i typowane kody błędów, z adapterem zachowującym v8 na granicy codec. |
| P1 | Server runtime | `apps/server/src/main.rs` posiada równolegle wiele map, kolejek, timerów i stanów. Wydzielić `ServerRuntime` oraz właścicieli `Connections`, `GameState`, `Persistence` z jawnie określonymi fazami. |
| P2 | Client | `apps/client/src/renderer.rs` przechowuje jednocześnie GPU, konsolę, input, predykcję, sesję, connector i lokalny proces. Zastosować kompozycję zasobów, nie kolejne przekazywanie szerokiego `&mut Renderer`. |
| P2 | Config | `ThreadLane`, `LaneSelection`, `WorkerSelection` mają podobne nazwy lecz inne dopuszczalne zakresy. Współdzielić wybór Fixed/Auto jedynie w razie użycia, a ograniczenia walidować per aplikacja. |
| P2 | Przykładowa gra | `core::world::SAMPLE_MAP` wciągało `games/sample` do domeny. W tym PR źródło assetu pozostaje jedno, ale włączają je binaria. |
| P2 | World inputs | Typ wejścia świata był zdefiniowany w `message_dispatch`, a aplikowany w `world_inputs`. Ten PR przenosi jego definicję do konsumenta/warstwy aplikacji. |

## Kolejność stabilizacji

1. **A0 (ten PR):** usunąć referencję core → `games/sample`, przenieść typ `WorldInput` do modułu jego aplikowania, zabezpieczyć inwariant indeksu po rejoin. Bez modyfikowania formatu protokołu i zapisów.
2. **A1 — czysty core:** wydzielić I/O profilów, endpoint/lock, admin client i file operations do infrastructure/runtime; oddzielić executor i telemetry od praw fizyki i modelu świata. Zaprojektować API i graf zależności przed przenoszeniem plików.
3. **A2 — typy:** `PlayerId`, `ServerErrorCode`, wyniki komend, konfiguracje; jeden właściciel typu w domenie. Bez globalnego `enums.rs` i bez zmiany bajtów protocol v8 / save v3.
4. **A3 — server:** własność stanu i jawne fazy `poll_network`, `apply_inputs`, `tick`, `replicate`, `shutdown`; zachować istniejącą kolejność, limity i semantykę kolejki persistence.
5. **A4 — client:** podzielić `Renderer` na `GpuResources`, `InputState`, `GameplayState`, `ConnectionState`; proces lokalnego serwera nadal należy do bootstrapu. Zachować wspólną implementację `ClientSession` w oknie i terminalu.
6. **A5 — weryfikacja:** formatowanie, kompilacja i analiza statyczna na kolejnych PR-ach; testy i pomiary zgodnie z końcową fazą z `AGENTS.md`.

## Kryteria

- Zachować protocol v8, snapshot v3, map fingerprint, komendy i dane użytkownika.
- Logika świata nie powinna posiadać socketów, administracji procesu, GPU ani zależności od przykładowej gry.
- Nie tworzyć pustych crate'ów ani abstrakcji/traitów bez co najmniej jednej wyraźnej granicy zależności.
- Zmiany prowadzić w małych PR-ach; każdą zmianę zachowania wyraźnie dokumentować.
