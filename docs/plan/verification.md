# Kompilacja i statyka — 2026-10-28

Środowisko: Windows, Rust1.99.0 (b940084d7 2026-09-28), workspace offline z Cargo.lock. Wyniki końcowe:

| Kontrola | Wynik |
|---|---|
| rtk cargo fmt --all -- --check | PASS |
| rtk cargo check --workspace --locked --offline | PASS,0 błędów/ostrzeżeń kodu |
| rtk cargo clippy --workspace --locked --offline | PASS,4 istniejące uwagi bazowe |
| rtk cargo build --workspace --release --locked --offline | PASS, wszystkie4 binaria |
| rtk cargo check -p minimium-server --features allocation-metrics --locked --offline | PASS |
| rtk cargo build -p minimium-server --release --features allocation-metrics --locked --offline | PASS |
| rtk git diff --check | PASS |
| Whitespace nowych nieśledzonych źródeł/dokumentów | PASS (git diff ich nie obejmuje) |
| Statyczne parsowanie18 manifestów TOML i zgodność parametrów/flag | PASS; nic nie uruchomiono |
| rtk cargo tree -p minimium-server --locked --offline | tylko core, protocol, telemetry; brak GPU/UI/audio |

Końcowy check/Clippy wykonano z CARGO_INCREMENTAL=0, ponieważ system plików nie pozwala na hardlinki cache. Nie wycisza to lintów ani ostrzeżeń kompilatora. Z cache widoczna była ta sama kategoria ostrzeżenia środowiskowego co w bazie; więcej miejsc wynika z nowego crate i build script. Cztery uwagi kodu pozostały: protocol large_enum_variant/type_complexity oraz console append_text/append_quad too_many_arguments. Brak nowych wyciszeń i nowych ostrzeżeń kodu. Logi lokalne: target/epic001/verification-0..4.log.

Na końcu odtworzono domyślny release workspace bez allocation-metrics. Wariant instrumentowany trzeba zbudować z feature przed pomiarem. Rust wersja przypięta; protokół v8 i zapis v3 zachowane. Fingerprint demo.map celowo inny, istniejące dane nietknięte.

Repo identity guard przed aktualizacją GitHub: PASS, origin clickbytes/minimium, autor potepapawel@hotmail.com, HTTPS Git potwierdzony przez credential helper GitHub CLI jako clickbytes. Nie wykonano commitów ani pushów.

Nie dodano ani nie uruchomiono unit/integration/e2e, procesów gry ani generatora obciążenia. Nie wykonano porównania lane’ów ani pomiaru sprzętu. Kompilacja hosta nie dowodzi działania Windows ani Linux; backend Linux był przedmiotem przeglądu kodu, bez cross-build.

Epic001 zaktualizowany w GitHub: https://github.com/clickbytes/minimium/issues/1 — pozostaje open, z rozdzieleniem odbioru implementacji i odroczonego działania. #6 i #11 nie zmieniano.
