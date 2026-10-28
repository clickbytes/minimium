# Epic 002 — baseline, 2026-10-28

Bazowy commit main: 53314514dac1456744af909aa4a1ea910458d111.
PR #13: 6fff2c4428d64a2fd69f5f57c3a20bfe439ee27e (draft, bez kompilacji).
Stan: **statyczne code review na zdalnym GitHub, brak wykonania komend kompilacji, testów, aplikacji i generatora**.

## Repo zastane

- crates/core: world/movement/collision/map, ale też executor zależny od telemetry, profile disk IO, endpoint, admin client, files i server lock.
- crates/protocol: framing v8, encoding/decoding, models z Error(u8) i raw player u64.
- crates/telemetry: process/allocations/duration.
- apps/server: main loop z rozproszonymi kolekcjami mutable, adaptery network/commands/storage.
- apps/client: session i connector współdzielone, lecz renderer ma GPU/connection/input/gameplay w jednym dużym strukturze.
- apps/ctl i apps/loadtest: admin client, raport v2, 18 manifestów.
- docs/architecture.md: rozdziela target i faktyczny stan, ale target crates nie są zaimplementowane.

## Kontrakty

Binary protocol v8, frame header 4 B, payload 4096 B, profile 16 B, visible players 16; save v3 (migracja v2), map fingerprint. Server tick 50 ms/snapshot 100 ms, bounded queues/handshakes, 16 KiB per client / 256 KiB per poll. Te wartości są wymaganiami do zachowania, nie wynikami testu.

## Ryzyka do obsłużenia

Core miesza domenę z I/O i telemetry; brakuje domain PlayerId i typed server error; wielkie state aggregaty; potencjalny stale world cell index przy join-at replacement naprawiany w PR #13; sample map w core (PR #13); Renderer modyfikuje gameplay i socket state; nieudowodniona wydajność i lane equivalence.

## Brak testów

Wszystkie bieżące wyniki testów i pomiarów: **NOT RUN / DEFERRED**. Poprzedni dokument refactor-review potwierdza tylko dawne static checks, nie wynik nowego epica. Zgodnie z AGENTS.md test phase dopiero po jawnym poleceniu użytkownika.
