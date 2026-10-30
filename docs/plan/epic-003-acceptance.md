# E003 — macierz odbioru E003-06..14

**Aktualizacja 2026-10-30:** E002 [PR #31](https://github.com/clickbytes/minimium/pull/31)
scalono do `main` jako `9c938cc596e851bdbc3154fb45a01260330f14d6`,
a E003 [PR #50](https://github.com/clickbytes/minimium/pull/50) po zmianie bazy
na `main` jako `3e86b3e5a04a152567fde244f9f3cba217307081`.
Po zmianie bazy diff #50 obejmował 10 commitów E003 i 104 pliki; GitHub zgłaszał
`CLEAN`/`MERGEABLE`. [PR #49](https://github.com/clickbytes/minimium/pull/49)
zamknięto jako zastąpiony przez #50. Nie utworzono osobnych issue ani PR-ów
etapów. Otwarte pozostają wizualny odbiór klienta, pomiar E002 na sprzęcie
klasy Pentium 500 MHz i formalna końcowa faza testowa E003. Merge nie jest
potwierdzeniem tych kontroli. Poniższy stan bramek i kolejność integracji
opisują sytuację sprzed decyzji o wcześniejszym merge.

Stan implementacji na gałęzi `epic/003-seeded-worlds`. Główny przyrost jest w
`91f42f1`, poprawka lokalizacji radialnej w `5e7f801`, a przegląd zbiorczy w
[PR #50](https://github.com/clickbytes/minimium/pull/50). Nie utworzono osobnych
issue ani PR-ów etapów. Zewnętrzna recenzja pozostaje otwarta.

| Etap | Obszary implementacji | Dowód i pozostała bramka |
| --- | --- | --- |
| E003-06 | `crates/sectors/src/lib.rs`, `apps/server/src/sector_streaming.rs`, `apps/client/src/renderer/sector_scene.rs` | Limit żądań, ticket, instalacja danych i kolizji oraz zwalnianie są w kodzie. Próby jednostkowe opisano w `epic-003.md`; formalny odbiór wymaga osobno uruchomionej fazy testowej. |
| E003-07 | `crates/core/src/world/rules.rs`, `crates/core/src/world/geometry.rs`, `apps/client/src/renderer/camera.rs` | Ruch radialny i zorientowana kapsuła są we wspólnej logice. Odbiór wymaga sprawdzenia ruchu klienta na planecie i w dungeonie. |
| E003-08 | `apps/server/src/world_registry.rs`, `apps/server/src/runtime/inputs.rs` | Osobne instancje i epoki członkostwa są zaimplementowane; scenariusz izolacji dwóch instancji jest częścią formalnej fazy testowej. |
| E003-09 | `crates/protocol/src/models.rs`, `crates/protocol/src/world_context.rs`, `apps/client/src/session/receive.rs` | Kontekst v9 niesie instancję, rewizję i epoch; odbiór wymaga ramek v9 oraz odrzucenia starego kontekstu i niezgodnego digestu. |
| E003-10 | `apps/server/src/world_registry.rs`, `apps/server/src/runtime/tick.rs`, `apps/client/src/renderer/session/message_handling.rs` | Prepare, ready, commit i cancel są w kodzie. Cel jest sprawdzany względem pełnej geometrii przed przydzieleniem epoki; formalny odbiór obejmuje transfer i powrót przez portal. |
| E003-11 | `apps/server/src/storage/format.rs`, `apps/server/src/storage/player_store.rs`, `crates/platform/src/files.rs` | Zapis v6, powrót, zmiany sektorów i migracja v2/v3 są zaimplementowane. W fazie testowej trzeba potwierdzić odczyt po restarcie i zachowanie pliku po błędzie zapisu. |
| E003-12 | `apps/client/src/renderer/sector_scene.rs`, `apps/client/src/renderer/gpu.rs`, `apps/client/src/renderer/session/message_handling.rs` | Kod przygotowuje kolizję i bufory celu przed ready. Brakuje odbioru renderowanego przejścia i odbudowy zasobów na działającym kliencie. |
| E003-13 | `crates/procgen/src/demo.rs`, `apps/server/src/startup.rs`, `apps/server/src/world_registry.rs` | Wcześniejsza próba TCP przeszła planetę, dungeon, powrót i reconnect. Formalny demonstrator musi zostać wykonany po poleceniu rozpoczęcia fazy testowej. |
| E003-14 | `docs/adr/0005-seeded-procgen.md`, `docs/worlds.md`, `scripts/check_architecture.py` | `fmt --check`, `cargo check`, Clippy, kontrola architektury, limit 500 linii i `git diff --check` przeszły. Zewnętrzna recenzja i integracja są otwarte. |

## Kolejność integracji

1. Rozwiązać otwarte bramki bazowego [PR #31](https://github.com/clickbytes/minimium/pull/31),
   który jest draftem i nadal bazuje na `main`.
2. Dokończyć przegląd kodu PR #50. Zapis o jednym zbiorczym PR jest jawnym
   odstępstwem od planu etapowych issue i PR-ów; nie przypisywać mu fikcyjnych
   recenzji ani osobnych SHA.
3. Po osobnym poleceniu użytkownika rozpocząć końcową fazę testową i wypełnić
   scenariusze z `epic-003.md`, w tym wizualne przejście klienta.
4. Po scaleniu E002 zmienić bazę PR #50 na aktualny `main`, ponownie ocenić diff,
   uzyskać recenzję i scalić. Dopiero merge SHA i wynik bramek zamykają epic.
