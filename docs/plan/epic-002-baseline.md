# Epic 002 — baseline, 2026-10-28

Bazowy commit main: 53314514dac1456744af909aa4a1ea910458d111.
PR #13: head ecffa5456afce42742a3996ee0b4cafa123b9bde, merge a3cba16f232a7967505376f84497350667f5c38e do gałęzi epica.
Stan: przegląd i kontrole statyczne wykonane; testy, aplikacja i generator nie były uruchamiane.

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

## Wyniki E002-00

Windows x86_64-pc-windows-msvc, rustc 1.99.0 (b940084d7 2026-09-28). Na bazie 53314514 oraz PR #13 (6fff2c4 z poprawką formatowania ecffa54): cargo fmt --all -- --check, cargo check --workspace --locked --offline, cargo clippy --workspace --locked --offline, cargo build --workspace --locked --offline oraz cargo build -p minimium-server --features allocation-metrics --locked --offline przeszły; fmt PR przed poprawką nie przeszedł. python scripts/check_file_lengths.py i git diff --check przeszły. Clippy zgłasza zastane large_enum_variant w protocol/models.rs i client/session.rs; cache incremental zgłasza brak hardlinków. Git zgłasza brak części plików commit-graph. Nie dodano alokacji dla wyciszenia ostrzeżeń.

Przegląd: leave usuwa wpis gracza i jego komórki przed replacement; sample map pozostaje jednym plikiem włączanym przez oba binaria, WorldInput należy do modułu aplikacji wejść. Zgodność runtime pozostaje nieweryfikowana.

Graf lokalnych zależności z cargo metadata --no-deps --locked --offline: core → telemetry; protocol i telemetry bez lokalnych zależności; server, client i loadtest → core, protocol, telemetry; ctl → core. Client dodatkowo zależy od bytemuck, glam, pollster, wgpu i winit.
