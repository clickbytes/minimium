# Baza 2026-10-28

Rust 1.99.0 (b940084d7 2026-09-28), Windows. Przed zmianami check workspace offline locked: 0 błędów, 6 ostrzeżeń środowiska o hardlinkach incremental cache. Clippy: 0 błędów, te 6 ostrzeżeń oraz 4 uwagi: large_enum_variant (protocol ServerMessage), type_complexity (protocol decode_frame), too_many_arguments (console append_text, append_quad). Nie dodajemy wyciszeń. Testów i programów nie uruchamiano.

Początkowy diff: README; client console/main/renderer; loadtest main; server main; core commands/executor/world; protocol lib; docs architecture/maps/performance/protocol; demo.map. Nieśledzone mesh_demo.map i models/ zachowane. Brak .codegraph, więc scoped rg/read.

