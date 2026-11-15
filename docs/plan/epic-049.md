# E049 — Spójni właściciele stanu i wspólne wykonywanie operacji

Aktywny epic: [#421](https://github.com/clickbytes/minimium/issues/421). Kontrakt O00: [#422](https://github.com/clickbytes/minimium/issues/422).
Bazowy checkout: `main@7d7ada8ca16ff52db28a889247e5b8173644b6d5`, czyste drzewo; repo-check potwierdził autora i Git transport jako clickbytes.
Przed każdym przydziałem odczytać aktualny main, rzeczywiste stany GitHub, diff i aktualnych producentów/konsumentów. Baseline nie jest poleceniem cofnięcia kodu.

## Obowiązujący workflow

Ten dokument i issues E049 zastępują instrukcje wykonywania historycznych E046/E047 i wcześniejszych sekwencyjnych planów. Ich historia i dowody pozostają zachowane. E047 #366 zamknięto jako **zastąpiony**, nie funkcjonalnie ukończony; niewykonany odbiór meteorów #384 jest dzieckiem Q03 #470.

Koordynator i maksymalnie trzech agentów **gpt-6-luna**. Każdy implementator pracuje nad jednym issue/PR na osobnej gałęzi i worktree; bez równoległej edycji wspólnego checkoutu. Koordynator prowadzi O00, S13, Q01 i Q02, ustala kontrakty, rezerwuje pliki i integruje pojedynczo. Wspólny mutowany kontrakt wyklucza równoległość; niezależne rozłączne zadania nie są globalnie serializowane. Nową zależność zgłosić przed rozszerzeniem zakresu.

Start zależnego issue wymaga odczytu **CLOSED wszystkich poprzedników**. Wszystkie implementacje zależą od O00. Pierwszy przydział: W01, C02, G01. Następnie pierwszeństwo mają gotowe zadania P1; wolne miejsce może zająć niezależne zadanie bez kolizji. Przekazany plan nie zawiera pełnej klasyfikacji P1; koordynator zapisuje bieżące priorytety, nie wymyśla historycznych ocen.

## Kontrakty przekrojowe

- Jeden właściciel mutable state; domena bez filesystem/network/process/GPU/UI effects poza zadeklarowanymi adapterami/aplikacjami.
- Wersja terenu ma osobne monotoniczne składniki; world/content identity pozostaje w kluczu/kontekście. Szersza invalidacja dopuszczona, utrata invalidacji przez XOR nie.
- Active content spójny podczas prepare; pending związany z ticket/context. Commit przełącza komplet, cancel/fail/disconnect usuwa pending, stale results nie publikują.
- Typed intent, owner validation i typed result. Enqueue nie oznacza autorytatywnego sukcesu; Started transferu nie oznacza commit.
- Podzbiór entity mutations w batch atomowy; wyniki pozostałych efektów jawne. Durable-before-publication dla wykopów pozostaje.
- Process i embedded korzystają z tego samego runtime owner. Host dostarcza usługi i czas, constructor nie uruchamia TCP/filesystem.
- Renderer konsumuje CPU views i zgłasza readiness. Session/application lifecycle należy do aplikacji/runtime.
- Monotonic lifecycle/scheduling i cosmetic time oddzielone od fixed simulation dt i UTC/world time.
- Zachować protokół v21, save v11, generator revision 6, content/ID/seed/fingerprint, asset bytes/digests i GPU ABI. Zamierzone naprawy tylko w konkretnych issues.
- Bez migration/legacy layers, alternatywnego executora, pustych crates i nieużywanych APIs. Przełączyć konsumentów i usunąć zastępowaną ścieżkę w jednym PR.

To **docelowe warunki odbioru**, nie deklaracja ukończenia. Faktyczną własność aktualizować w architecture.toml i README razem z implementacją.

## Graf zależności i pokrycie ustaleń

| Issue | Ustalenia | Poprzednicy (CLOSED przed startem) |
|---|---|---|
| [O00 #422](https://github.com/clickbytes/minimium/issues/422) — Kontrakt wykonania epiku | — | — |
| [W01 #423](https://github.com/clickbytes/minimium/issues/423) — Jednoznaczna wersja zawartości terenu | F01 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [W02 #424](https://github.com/clickbytes/minimium/issues/424) — Wspólna publikacja geometrii przestrzennej | F02 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [W01 #423](https://github.com/clickbytes/minimium/issues/423) |
| [W03 #425](https://github.com/clickbytes/minimium/issues/425) — Wspólna geometria postaci | F03 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [W02 #424](https://github.com/clickbytes/minimium/issues/424) |
| [C01 #426](https://github.com/clickbytes/minimium/issues/426) — Jeden katalog wartości ustawień | F06,F36 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [C02 #427](https://github.com/clickbytes/minimium/issues/427) — Rozdzielenie aktywnego i przygotowywanego contentu | F04 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [C03 #428](https://github.com/clickbytes/minimium/issues/428) — Stan połączenia pilnowany przez właściciela | F34 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [C04 #429](https://github.com/clickbytes/minimium/issues/429) — Wspólny kontroler gotowości transferu | F05 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [C02 #427](https://github.com/clickbytes/minimium/issues/427), [C03 #428](https://github.com/clickbytes/minimium/issues/428) |
| [C05 #430](https://github.com/clickbytes/minimium/issues/430) — Zgodny katalog kontrolek i obsługa myszy | F35 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [C06 #431](https://github.com/clickbytes/minimium/issues/431) — Wspólny ClientBootstrap | F19 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [C01 #426](https://github.com/clickbytes/minimium/issues/426), [C03 #428](https://github.com/clickbytes/minimium/issues/428) |
| [C07 #432](https://github.com/clickbytes/minimium/issues/432) — Typowany executor akcji klienta | F11 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [C05 #430](https://github.com/clickbytes/minimium/issues/430), [C06 #431](https://github.com/clickbytes/minimium/issues/431) |
| [C08 #433](https://github.com/clickbytes/minimium/issues/433) — Komendy bez widgetu konsoli | F12 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [C07 #432](https://github.com/clickbytes/minimium/issues/432) |
| [C09 #434](https://github.com/clickbytes/minimium/issues/434) — Samodzielna aplikacja klienta | F08 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [C04 #429](https://github.com/clickbytes/minimium/issues/429), [C06 #431](https://github.com/clickbytes/minimium/issues/431), [C08 #433](https://github.com/clickbytes/minimium/issues/433), [P01 #459](https://github.com/clickbytes/minimium/issues/459) |
| [C10 #435](https://github.com/clickbytes/minimium/issues/435) — Krok aplikacji niezależny od redraw | F18 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [C09 #434](https://github.com/clickbytes/minimium/issues/434) |
| [S01 #436](https://github.com/clickbytes/minimium/issues/436) — Obowiązkowa walidacja komponentów | F20 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [S02 #437](https://github.com/clickbytes/minimium/issues/437) — Rejestracja impact przez silnik | F23 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [S01 #436](https://github.com/clickbytes/minimium/issues/436) |
| [S03 #438](https://github.com/clickbytes/minimium/issues/438) — Jeden właściciel live impact state | F21 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [S02 #437](https://github.com/clickbytes/minimium/issues/437), [S04 #439](https://github.com/clickbytes/minimium/issues/439) |
| [S04 #439](https://github.com/clickbytes/minimium/issues/439) — Neutralne zmiany i rezerwacja storage | F24 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [S01 #436](https://github.com/clickbytes/minimium/issues/436) |
| [S05 #440](https://github.com/clickbytes/minimium/issues/440) — Wynik każdego efektu | F22 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [S04 #439](https://github.com/clickbytes/minimium/issues/439) |
| [S06 #441](https://github.com/clickbytes/minimium/issues/441) — Autorytatywny właściciel reguł akcji | F16 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [W03 #425](https://github.com/clickbytes/minimium/issues/425) |
| [S07 #442](https://github.com/clickbytes/minimium/issues/442) — Typowane wyniki administracji | F14 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [S05 #440](https://github.com/clickbytes/minimium/issues/440) |
| [S08 #443](https://github.com/clickbytes/minimium/issues/443) — Użyteczny port komend modułów | F13 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [S05 #440](https://github.com/clickbytes/minimium/issues/440), [S06 #441](https://github.com/clickbytes/minimium/issues/441), [S07 #442](https://github.com/clickbytes/minimium/issues/442) |
| [S09 #444](https://github.com/clickbytes/minimium/issues/444) — Harmonogram obejmujący built-ins | F15 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [S08 #443](https://github.com/clickbytes/minimium/issues/443) |
| [S10 #445](https://github.com/clickbytes/minimium/issues/445) — Właściciel transakcji wykopów | F09 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [W02 #424](https://github.com/clickbytes/minimium/issues/424), [S03 #438](https://github.com/clickbytes/minimium/issues/438), [S04 #439](https://github.com/clickbytes/minimium/issues/439), [S05 #440](https://github.com/clickbytes/minimium/issues/440) |
| [S11 #446](https://github.com/clickbytes/minimium/issues/446) — Transport jako adapter hosta | F09,F10 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [S06 #441](https://github.com/clickbytes/minimium/issues/441), [S08 #443](https://github.com/clickbytes/minimium/issues/443) |
| [S12 #447](https://github.com/clickbytes/minimium/issues/447) — Jawny port persistence | F09,F10 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [S10 #445](https://github.com/clickbytes/minimium/issues/445), [S11 #446](https://github.com/clickbytes/minimium/issues/446) |
| [S13 #448](https://github.com/clickbytes/minimium/issues/448) — Publiczne submit/query/step | F10 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [S09 #444](https://github.com/clickbytes/minimium/issues/444), [S12 #447](https://github.com/clickbytes/minimium/issues/447) |
| [S14 #449](https://github.com/clickbytes/minimium/issues/449) — Jawna polityka czasu pracy asynchronicznej | F17 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [S10 #445](https://github.com/clickbytes/minimium/issues/445), [S13 #448](https://github.com/clickbytes/minimium/issues/448) |
| [S15 #450](https://github.com/clickbytes/minimium/issues/450) — Deadline i cleanup transferu | F05 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [C04 #429](https://github.com/clickbytes/minimium/issues/429), [S05 #440](https://github.com/clickbytes/minimium/issues/440) |
| [G01 #451](https://github.com/clickbytes/minimium/issues/451) — Ochrona instalowanego pakietu przed ewikcją | F07 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [G02 #452](https://github.com/clickbytes/minimium/issues/452) — Poprawna konstrukcja i błędy próbkowania Earth | F29 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [G03 #453](https://github.com/clickbytes/minimium/issues/453) — Polityka scenariusza dostarczana przez grę | F25 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [G02 #452](https://github.com/clickbytes/minimium/issues/452), [G04 #454](https://github.com/clickbytes/minimium/issues/454) |
| [G04 #454](https://github.com/clickbytes/minimium/issues/454) — Wspólny profil materiałowy terenu | F27 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [W01 #423](https://github.com/clickbytes/minimium/issues/423), [G02 #452](https://github.com/clickbytes/minimium/issues/452) |
| [G05 #455](https://github.com/clickbytes/minimium/issues/455) — Neutralna operacja deformacji | F26 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [S06 #441](https://github.com/clickbytes/minimium/issues/441), [G04 #454](https://github.com/clickbytes/minimium/issues/454) |
| [G06 #456](https://github.com/clickbytes/minimium/issues/456) — Modele narzędzi jako dane gry | F26 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [G05 #455](https://github.com/clickbytes/minimium/issues/455), [P01 #459](https://github.com/clickbytes/minimium/issues/459) |
| [G07 #457](https://github.com/clickbytes/minimium/issues/457) — Niezmienna publikacja materiałów | F38 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [G08 #458](https://github.com/clickbytes/minimium/issues/458) — Jawny krok cook i manifest pakowania | F39 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [G07 #457](https://github.com/clickbytes/minimium/issues/457) |
| [P01 #459](https://github.com/clickbytes/minimium/issues/459) — Kontrakty CPU bez feature GPU | F28 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [P02 #460](https://github.com/clickbytes/minimium/issues/460) — Czas prezentacji dostarczany przez hosta | F30 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [P01 #459](https://github.com/clickbytes/minimium/issues/459), [C10 #435](https://github.com/clickbytes/minimium/issues/435) |
| [P03 #461](https://github.com/clickbytes/minimium/issues/461) — Neutralne transient lights | F31 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [P01 #459](https://github.com/clickbytes/minimium/issues/459) |
| [P04 #462](https://github.com/clickbytes/minimium/issues/462) — Walidacja uploadu chmur | F33 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [P05 #463](https://github.com/clickbytes/minimium/issues/463) — Widoczność zdalnych graczy | F37 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [P06 #464](https://github.com/clickbytes/minimium/issues/464) — Hermetyczny WeatherModel | F32 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [P07 #465](https://github.com/clickbytes/minimium/issues/465) — Debug i LOD chmur w prezentacji | F41 | [O00 #422](https://github.com/clickbytes/minimium/issues/422), [P06 #464](https://github.com/clickbytes/minimium/issues/464), [C01 #426](https://github.com/clickbytes/minimium/issues/426) |
| [P08 #466](https://github.com/clickbytes/minimium/issues/466) — Uzgodniona tożsamość ciał niebieskich | F42 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [P09 #467](https://github.com/clickbytes/minimium/issues/467) — Domenowy błąd parsowania profilu | F43 | [O00 #422](https://github.com/clickbytes/minimium/issues/422) |
| [Q01 #468](https://github.com/clickbytes/minimium/issues/468) — Egzekwowalne granice | F40 | [W01 #423](https://github.com/clickbytes/minimium/issues/423), [W02 #424](https://github.com/clickbytes/minimium/issues/424), [W03 #425](https://github.com/clickbytes/minimium/issues/425), [C01 #426](https://github.com/clickbytes/minimium/issues/426), [C02 #427](https://github.com/clickbytes/minimium/issues/427), [C03 #428](https://github.com/clickbytes/minimium/issues/428), [C04 #429](https://github.com/clickbytes/minimium/issues/429), [C05 #430](https://github.com/clickbytes/minimium/issues/430), [C06 #431](https://github.com/clickbytes/minimium/issues/431), [C07 #432](https://github.com/clickbytes/minimium/issues/432), [C08 #433](https://github.com/clickbytes/minimium/issues/433), [C09 #434](https://github.com/clickbytes/minimium/issues/434), [C10 #435](https://github.com/clickbytes/minimium/issues/435), [S01 #436](https://github.com/clickbytes/minimium/issues/436), [S02 #437](https://github.com/clickbytes/minimium/issues/437), [S03 #438](https://github.com/clickbytes/minimium/issues/438), [S04 #439](https://github.com/clickbytes/minimium/issues/439), [S05 #440](https://github.com/clickbytes/minimium/issues/440), [S06 #441](https://github.com/clickbytes/minimium/issues/441), [S07 #442](https://github.com/clickbytes/minimium/issues/442), [S08 #443](https://github.com/clickbytes/minimium/issues/443), [S09 #444](https://github.com/clickbytes/minimium/issues/444), [S10 #445](https://github.com/clickbytes/minimium/issues/445), [S11 #446](https://github.com/clickbytes/minimium/issues/446), [S12 #447](https://github.com/clickbytes/minimium/issues/447), [S13 #448](https://github.com/clickbytes/minimium/issues/448), [S14 #449](https://github.com/clickbytes/minimium/issues/449), [S15 #450](https://github.com/clickbytes/minimium/issues/450), [G01 #451](https://github.com/clickbytes/minimium/issues/451), [G02 #452](https://github.com/clickbytes/minimium/issues/452), [G03 #453](https://github.com/clickbytes/minimium/issues/453), [G04 #454](https://github.com/clickbytes/minimium/issues/454), [G05 #455](https://github.com/clickbytes/minimium/issues/455), [G06 #456](https://github.com/clickbytes/minimium/issues/456), [G07 #457](https://github.com/clickbytes/minimium/issues/457), [G08 #458](https://github.com/clickbytes/minimium/issues/458), [P01 #459](https://github.com/clickbytes/minimium/issues/459), [P02 #460](https://github.com/clickbytes/minimium/issues/460), [P03 #461](https://github.com/clickbytes/minimium/issues/461), [P04 #462](https://github.com/clickbytes/minimium/issues/462), [P05 #463](https://github.com/clickbytes/minimium/issues/463), [P06 #464](https://github.com/clickbytes/minimium/issues/464), [P07 #465](https://github.com/clickbytes/minimium/issues/465), [P08 #466](https://github.com/clickbytes/minimium/issues/466), [P09 #467](https://github.com/clickbytes/minimium/issues/467) |
| [Q02 #469](https://github.com/clickbytes/minimium/issues/469) — Końcowy przegląd integracyjny | — | [Q01 #468](https://github.com/clickbytes/minimium/issues/468) |
| [Q03 #470](https://github.com/clickbytes/minimium/issues/470) — Final testing i odbiór E049 | — | [Q02 #469](https://github.com/clickbytes/minimium/issues/469) |

Szczegółowe kontrakty, zakresy i scenariusze: issues oraz [dane backlogu](epic-049-backlog.json). JSON jest dokumentacją wygenerowaną z zatwierdzonego planu, nie kodem wykonawczym. F01–F43 pochodzą z przekazanego planu; nie deklarujemy dodatkowego lokalnego raportu.

F05: C04/S15; F09: S10–S12; F10: S11–S13; F26: G05/G06; reszta według tabeli. Q01 czeka na wszystkie 45 implementacji, Q02 na Q01, Q03 na Q02 i jawne rozpoczęcie final testing.

## Rezerwacje początkowe po O00 CLOSED

| Issue | Zakres wyłączny | Wyłączenia / handoff |
|---|---|---|
| W01 #423 | TerrainInputs/version, revision producers, sector/render cache consumers | Bez active/pending content i cache dyskowego. |
| C02 #427 | ClientSession content, receive/decode, terrain reads i desktop content access | Bez reprezentacji TerrainInputs/version; wspólne pliki streaming uzgodnić przed edycją. |
| G01 #451 | ContentCache install/make_room i README asset-io | Bez nowego lock protocol, zmiany content bytes i material server snapshot G07. |

Każdy agent odczytuje własne AGENTS, README i aktualne callsites. Wspólne wpisy architecture.toml scala koordynator po review; kolizję pliku/kontraktu należy zgłosić przed edycją.

## Weryfikacja, integracja i sprzątanie

1. Agent dostarcza PR z **Refs**, bez auto-close, dokumentację i dowody SHA/commands/exit codes.
2. Koordynator sprawdza diff, negative/cancel/stale/partial paths, feature gates, konsumentów i usunięcie starej ścieżki.
3. Integruje pojedynczo do aktualnego main po spełnieniu kryteriów.
4. Odczytuje main/merge SHA; po zmianie sprawdzonego drzewa ponawia dotknięte kontrole.
5. Agent zapisuje merge SHA i dowody, zamyka własne issue i ponownie odczytuje CLOSED.
6. Agent sprząta wyłącznie własne zasoby/worktree po sprawdzeniu braku niezintegrowanej pracy; koordynator usuwa zintegrowaną gałąź. Stare/user worktree poza zakresem.
7. Dopiero wtedy odblokowują się następcy.

Przez RTK: git diff --check; cargo fmt --all -- --check; scoped cargo check i workspace --locked; client --no-default-features; renderer CPU/examples i embedded host gdy dotknięte; architecture/file_lengths/static_refactor/agent_contracts; istniejący WGSL validator przy shaderach/layouts. Raport: PASS/FAIL/NOT RUN/NOT APPLICABLE, exit code, SHA. Nie powtarzać kosztownych kontroli bez nowej zmiany/przyczyny. Regex guard nie dowodzi ownership; kompilacja nie dowodzi runtime.

## Osobna bramka odbioru

**Q03 #470 i meteor #384 pozostają OPEN** do jawnego rozpoczęcia final testing przez użytkownika. Bez unit/integration/E2E suites, gry, GPU sessions i benchmarków. Zapisywać scenariusze, nie oznaczać ich PASS.

Macierz: równoczesne zmiany chunków i movement/tool/hitscan; radialna postać/kamera/cień; transfer obu hostów prepare/ready/commit/cancel/timeout/late; settings/reset/MouseRight/stdin; cache quota/leases/source overwrite; invalid/system components, partial effects/permissions/backpressure; process/embedded parity; formats/save/seeds/digests; CPU build/hidden scene/cloud limits; pełne meteory #384.

Epic otwarty do wszystkich dzieci CLOSED, końcowego odbioru i main verification. Refaktor/static checks raportować osobno od testów/runtime.
