# Epic E046 — Dokończenie fundamentów silnika

Status: issues 01–17 mają ukończony przegląd źródłowy; E046-17 zaliczył końcowe buildy i kontrole statyczne. E046-18 przygotowuje mapę dowodów i backlog runtime. Issues GitHub pozostają otwarte zgodnie z planem. Data review: 2026-11-07.

Epic GitHub: https://github.com/clickbytes/minimium/issues/347. Zadania wykonawcze: #348–#365. Dokument lokalny odzwierciedla wykonanie; issue i epic na GitHubie pozostają otwarte.

Cel: dokończyć pierwotny plan 01–47, naprawić rozbieżności znalezione w kodzie, następnie wykonać wyłącznie przewidziane kompilacje i walidację statyczną. Ten dokument jest indeksem epicu; pełne instrukcje wykonawcze znajdują się w plikach issues podlinkowanych poniżej. Opisy issues i ich implementacja są już opublikowane; statusów GitHub nie zmieniać w ramach tego planu.

## Wniosek review

Kroki 01–45 mają lokalne dowody przeglądu źródłowego w `docs/architecture-execution.md` i raportach E046-14–16; E046-16 uzgodnił dokumentację oraz macierz wymagań, właścicieli, konsumentów i dowodów. E046-17 potwierdził końcowe buildy i kontrole statyczne kroku 46. Zachowanie runtime nie zostało sprawdzone. Krok 26, wcześniej oznaczony In progress, ma source-review dowód dla pełnego zapisu/odtworzenia encji i znacznika inicjalizacji. Scenariusze runtime pozostają osobną, późniejszą fazą.

Review obejmowało odczyt kodu aplikacji, predykcji, admission ruchu, harmonogramu gameplay, encji, replikacji, fontu, zasobów GPU i skryptów architektury oraz dokumentacji wykonania. Buildy i kontrole z E046-17 przeszły; testów, gry, loadtestu i benchmarków nie uruchomiono. Poprawność runtime i wydajność nadal nie są dowiedzione.

## Zasady wspólne dla każdego issue

- Zachować ruch, walkę, czat, portale, proceduralne planety, Earth, dungeon, światy autorskie, klienta desktop i terminal.
- Przeczytać `architecture.toml`, README właściciela i producentów/konsumentów zmienianego kontraktu. Istniejących zmian nie resetować; `concat.txt` pozostawić nietknięty.
- Nie dodawać migracji, kompatybilności v17 ani atrap. Jedyne docelowe wersje: protokół 18, zapis 10, generator 5.
- Do bramki E046-17 dozwolone są odczyty, wyszukiwania i przegląd diffu. Nie uruchamiać cargo check/build/test, Clippy, gry, WGSL ani skryptów statycznych wcześniej.
- Pliki kodu/shaderów do 500 fizycznych linii. Wyjątek wymaga konkretnego uzasadnienia i zakresu; nie ścieśniać instrukcji w jedną linię.
- Zamknięcie issue wymaga wskazania konkretnych implementacji, połączonych callsite’ów, usuniętej starej ścieżki i aktualizacji dokumentacji. Stan „kod przejrzany” pozostaje oddzielny od „kompilacja wykonana” i „zachowanie sprawdzone”.
- Issues audytowe kończą się tabelą każdego wymagania oryginalnego kroku: dowód w kodzie albo konkretna poprawka. Sam brak wyników grep lub obecność typu nie wystarcza.
- Nie zmieniać kont Git ani nie commitować. Kontynuować lokalną implementację kolejno po ukończeniu source review każdego issue; nie zmieniać statusów GitHub w ramach tego planu.

## Pokrycie oryginalnego planu

| Kroki | Odpowiedzialne issues | Stan dowodu |
|---|---|---|
| 01–03 | E046-01 | Source review: ścieżki właściwości i dzielniki cienia; WGSL i wygląd oczekują na E046-17/runtime |
| 04–08 | E046-02 | Source review: ścieżki, logowanie, retencja, startup status i typed errors |
| 09–10 | E046-03 | Source review: manifest revision 5, Generated/Authored i wspólny resolver |
| 11–16 | E046-04 | Source review: sektory, kolizje, cache i HTTP; runtime/pressure nieuruchomione |
| 17–20 | E046-05 | Source review: Principal, permissions, admission i protokół 18 |
| 21–22 | E046-06 | Source reasoning: bezpieczny ack i replay; ręczne ślady nie były runtime testami |
| 23–26 | E046-07 | Source review: budżety i cykl spawn/update/remove/despawn/save/load |
| 27–29 | E046-08 | Source review: tick, Startup lifecycle, system ordering/effects i portal moduł |
| 30–32 | E046-09 | Source review: bounded journal i atomowe full/delta replicas |
| 33 | E046-10 | Source review: wspólny shutdown deadline i final save |
| 34–37 | E046-11 | Source review: PL/EN, font, IME, DPI i input contexts |
| 38–39 | E046-12 | Source review: `SettingsStore` i application-owned transitions/game state |
| 40 | E046-13 | Source review: CPU shadow preparation/GPU upload split; visual outcome unknown |
| 41 | E046-14 | Source review: shared geometry publication stamps; rendering unverified |
| 42–44 | E046-15 | Source review: removed legacy paths, architecture/static checker and file lengths |
| 45 | E046-16 | Source review complete; documentation reconciled and evidence matrix recorded |
| 46 | E046-17 | Finalne buildy i kontrole statyczne zakończone pomyślnie; szczegóły w raporcie E046-17 |
| 47 | E046-18 | Raport końcowy mapuje dowody 01–47, kontrakty, usunięte ścieżki i oddzielny backlog runtime |

Kolejność: issues 01–16 po krokach źródłowych, następnie E046-17 (wyłącznie wymienione końcowe buildy/kontrole), a potem E046-18 (raport i oddzielny backlog runtime). Każde issue obejmuje wykonanie znalezionych poprawek. Zależności wymagają implementacji i review źródłowego; runtime evidence nie tworzy blokującego cyklu. Nie otwierać kompilacji, dopóki kwestie implementacyjne i dokumentacyjne nie zostaną rozstrzygnięte.

## Instrukcja rozpoczęcia pracy i pełne opisy issues

Pracuj na lokalnym drzewie D:\Git\minimium, gałąź `codex/epic-008-config-cli-console`, HEAD z review `f08e8a9cc385ccf3f97e3abc4622abf0703c0a57`, wraz z niezacommitowanymi zmianami. Sam checkout SHA nie zawiera całości opisywanej implementacji. Nie resetuj drzewa. Bieżąca praca wznowiona jest przy E046-16; przed przejściem dalej zapisz macierz dowodów do kroków 01–45.

Każde issue poniżej zawiera własne ograniczenia, pliki wejściowe, numerowaną instrukcję i checklistę raportu. Sekcje w dalszej części tego epicu stanowią skrót zakresu; wykonanie opiera się na pełnych opisach:

- [x] [E046-01 — Uzgodnić stan początkowy i potwierdzić naprawy numeryczne](https://github.com/clickbytes/minimium/issues/348) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-01.md).
- [x] [E046-02 — Domknąć audyt ścieżek, diagnostyki i modelu błędów](https://github.com/clickbytes/minimium/issues/349) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-02.md).
- [x] [E046-03 — Potwierdzić jeden kontrakt światów i resolvera](https://github.com/clickbytes/minimium/issues/350) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-03.md).
- [x] [E046-04 — Udowodnić ograniczony cykl pracy sektorów, kolizji, cache i HTTP](https://github.com/clickbytes/minimium/issues/351) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-04.md).
- [x] [E046-05 — Zweryfikować autoryzację każdego wejścia mutującego](https://github.com/clickbytes/minimium/issues/352) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-05.md).
- [x] [E046-06 — Naprawić semantykę ack i zgodność historii predykcji](https://github.com/clickbytes/minimium/issues/353) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-06.md).
- [x] [E046-07 — Domknąć trwałe encje i rzeczywistą rezerwację zapisu](https://github.com/clickbytes/minimium/issues/354) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-07.md).
- [x] [E046-08 — Uzupełnić kontrakt systemów gameplay i lifecycle świata](https://github.com/clickbytes/minimium/issues/355) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-08.md).
- [x] [E046-09 — Dokończyć audyt transakcji i ograniczeń replikacji](https://github.com/clickbytes/minimium/issues/356) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-09.md).
- [x] [E046-10 — Potwierdzić shutdown z rzeczywistym wynikiem zapisu](https://github.com/clickbytes/minimium/issues/357) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-10.md).
- [x] [E046-11 — Zamknąć pokrycie PL/EN, fontu i wejścia](https://github.com/clickbytes/minimium/issues/358) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-11.md).
- [x] [E046-12 — Dokończyć SettingsStore i rzeczywiste wydzielenie aplikacji](https://github.com/clickbytes/minimium/issues/359) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-12.md).
- [x] [E046-13 — Zakończyć rozdział przygotowania CPU i publikacji GPU](https://github.com/clickbytes/minimium/issues/360) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-13.md).
- [x] [E046-14 — Powiązać castery i odbiorców z tą samą publikacją geometrii](https://github.com/clickbytes/minimium/issues/361) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-14.md).
- [x] [E046-15 — Usunąć stare ścieżki i naprawić kontrolę architektury](https://github.com/clickbytes/minimium/issues/362) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-15.md).
- [x] [E046-16 — Uzgodnić dokumentację ze stanem kodu](https://github.com/clickbytes/minimium/issues/363) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-16.md).
- [ ] [E046-17 — Wykonać jedyną końcową fazę kompilacji i walidacji](https://github.com/clickbytes/minimium/issues/364) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-17.md).
- [ ] [E046-18 — Zamknąć epic raportem i oddzielnym backlogiem uruchomieniowym](https://github.com/clickbytes/minimium/issues/365) — pełna instrukcja i zależności w issue; [kopia lokalna](architecture-issues/E046-18.md).

## E046-01 — Uzgodnić stan początkowy i potwierdzić naprawy numeryczne

Priorytet P1. Kroki 01–03. Właściciel: aplikacja klienta i renderer. Zależności: brak.

Gdzie: `docs/architecture-execution.md`, `apps/client/src/terminal_client/properties.rs`, `apps/client/src/client_commands/properties/`, `crates/renderer/src/shaders/shadow_sampling.wgsl`.

Zakres:

- Ponownie odczytać RTK i lokalne instrukcje. Sprawdzić status repo i obecność indeksu CodeGraph; nie zakładać jego braku na podstawie starego raportu.
- Dla każdej właściwości wskazać ścieżkę czasu, ruchu lub prezentacji. Brak sesji/czasu ma być kontrolowany; funkcja prezentacji nie może otrzymać właściwości runtime przez fallback.
- Dla `receiver_depth_gradient` prześledzić wszystkie dzielenia i normalizacje. Degenerację normalnej/osi odrzucić przed normalizacją; przy ustawieniu <=0.005 zwracać zero przed dzieleniem przez nz, zachować fade do 0.05.
- Potwierdzić zabezpieczenie slope, 9 próbek PCF, środek texela i dotychczasowy world-space bias. Nie deklarować usunięcia migotania.

Akceptacja: tabela wszystkich kategorii właściwości i ścieżek numerycznych z odniesieniami do kodu; ewentualne poprawki połączone z konsumentami. WGSL pozostaje do E046-17, wygląd do osobnej fazy uruchomieniowej.

## E046-02 — Domknąć audyt ścieżek, diagnostyki i modelu błędów

Priorytet P1. Kroki 04–08. Właściciele: platform, diagnostics, telemetry, adaptery i entrypointy. Zależność: E046-01.

Gdzie: `crates/platform/src/app_paths.rs`, `startup_status.rs`, `crates/diagnostics/`, `crates/telemetry/src/logging/`, `apps/{client,server,ctl,loadtest}/src`, `crates/client-runtime/src/bootstrap/`, błędy w asset-io, storage i runtime’ach.

Zakres:

- Prześledzić pojedyncze rozstrzygnięcie ścieżek i przejście absolutnych danych/logów do dziecka. Wykluczyć ponowny odczyt środowiska w runtime. Zweryfikować `--log-dir` i domyślne ./logs.
- Prześledzić rekord 8 KiB/32 pól wraz z narzutem serializacji, `truncated`, kontrolowanym rejestrem domen oraz brakiem nieograniczonej alokacji przed obcięciem.
- Sprawdzić UTC dnia uruchomienia, atomową rezerwację numeru, parent_run_id, osobne kolejki 4096/128, nieblokujące emit, podsumowanie strat i propagację awarii writera.
- Udowodnić rotację 16 MiB + dwa segmenty oraz retencję 7 dni/2 GiB tylko własnych zakończonych uruchomień chronionych rzeczywistą blokadą.
- Dla wszystkich domen z planu wskazać podłączony logger. Oddzielić stdout CLI od diagnostyki. Prześledzić brak credentiali i pełnego czatu w polach.
- Startup musi weryfikować attempt ID i stan starting/ready/failed; stderr pozostaje ograniczonym fallbackiem awarii przed publikacją statusu.
- Prześledzić source i stabilne kategorie błędów aż do UI/logowania; nie usuwać semantyki na pośrednim `.to_string()`, `.ok()` czy ignorowanym Result.

Akceptacja: każdy limit i procesowy konsument ma konkretny dowód; błędy writera i statusu docierają do właściciela; żadna domena nie wykonuje zapisu logu. Brak dowodu staje się poprawką w tym issue, nie deklaracją sukcesu.

## E046-03 — Potwierdzić jeden kontrakt światów i resolvera

Priorytet P1. Kroki 09–10. Właściciele: procgen, sample, client-runtime, aplikacje. Zależność: E046-02.

Gdzie: `crates/procgen/src/{generator_contract,planet,materialize}.rs`, `games/sample/src/{scenario,planet_config}.rs`, `apps/client/src/world_content.rs`, `crates/client-runtime/src/world_content.rs`, `session/receive/`, `apps/client/src/renderer/session/world_install.rs`, `apps/server/src/composition.rs`.

Zakres: prześledzić rewizję 5 i odrzucanie innych rewizji przed generowaniem; jawne Generated/Authored i digest; oddzielne tryby proceduralny/Earth/dungeon; brak XOR-odtwarzania seedów, specjalnych ID 1/2 i legacy world injection. Wskazać jeden resolver desktop/terminal, walidację manifestu/fingerprintu/ticketu oraz błąd braku mapy. Usunąć pozostałą równoległą rekonstrukcję katalogu. Sprawdzić zależności produkcyjne runtime’ów od sample także przechodnio po zakończeniu implementacji.

Akceptacja: każdy rodzaj świata ma jednego producenta i jedną ścieżkę odtworzenia; stary manifest nie jest reinterpretowany; brak treści kończy operację błędem. Nie zmieniać algorytmu aktualnego świata.

## E046-04 — Udowodnić ograniczony cykl pracy sektorów, kolizji, cache i HTTP

Priorytet P1. Kroki 11–16. Właściciele: sectors, client/server-runtime, asset-io. Zależność: E046-03.

Gdzie: `crates/sectors/src`, `crates/client-runtime/src/session/content.rs`, `apps/client/src/renderer/{sector_worker,collision_worker,sector_scene}.rs`, `crates/server-runtime/src/sector_streaming/`, `crates/asset-io/src/{geography,content_cache,material_server}.rs`, `crates/server-runtime/src/environment/climate_config.rs`.

Zakres:

- Dla każdego begin/submit wskazać sukces, błąd, anulowanie, odzyskanie limitu i odrzucenie starego ticketu. Brak mapy, zerwany kanał i spawn failure nie mogą zostawić Generating.
- Poll klienta nie może wykonywać hash/mmap/materializacji. Utrzymać jedną aktywną pracę i jedno najnowsze oczekujące żądanie, anulowanie porcjami, 300 s i współdzielone Arc geografii.
- Serwer: 1–16 workerów, domyślnie min(max(parallelism-1,1),4), 64 zadania, do 8 instalacji/tick, brak synchronicznego fallbacku.
- Kolizja: niezmienny zestaw, rewizja źródła, scalanie najnowszej pracy i atomowa instalacja; ruch dopiero przy zgodnych rewizjach geometrii/kolizji.
- Cache: 8 GiB domyślnie, rezerwacja również na temp, lease, współdzielenie digestu, blokady między procesami, run/task w nazwie, atomowy plik i receipt. Nigdy usuwać źródłowych plików użytkownika.
- HTTP: 4 workery, kolejka 32, nagłówek 8 KiB/5 s, idle 15 s, transfer 300 s, allowlista plików i kontrolowane przepełnienie; klimat poza adapterem HTTP.

Akceptacja: diagramy lub tabele wszystkich stanów końcowych i limitów wskazują istniejące ścieżki kodu. Brak wiecznego Generating i nieograniczonego buforowania. Runtime/scenariusze awarii pozostają odroczone.

## E046-05 — Zweryfikować autoryzację każdego wejścia mutującego

Priorytet P0. Kroki 17–20. Właściciele: core, protocol, server-runtime i kompozycja aplikacji. Zależność: E046-04.

Gdzie: `apps/server/src/developer_auth.rs`, `crates/server-runtime/src/authentication.rs`, `runtime/admission/`, `crates/core/src/identity.rs`, `crates/protocol/src`, `crates/simulation/src/simulation/`, launchery i konfiguracja serwera.

Zakres: sporządzić macierz wiadomość → stan admission → permission → mutacja. Potwierdzić nieblokujące begin/poll/cancel, 10 s, jedną próbę/połączenie, Principal przed profilem i wykluczenie aktywnego profilu. Zwykły klient ma tylko gameplay; dev jawnie z --dev, także lokalny host i launchery. Lot, polityka ruchu, teleport i czas muszą być autoryzowane przed mutacją. Kanał admin zachowuje ochronę zapisu/stop. Wire ma jedną wersję 18, kontekst świata i payload <=4096. MovementIntent nie zawiera autorytatywnej prędkości/prawa do lotu; finiteness, osie i długość są walidowane, polityka serwerowa ma rewizję.

Akceptacja: kompletna macierz obejmuje desktop, terminal i loadtest; żadna mutacja nie zależy od ukrycia przycisku. Provider developerski pozostaje jawnie niezabezpieczoną deklaracją profilu, bez kont/TLS.

## E046-06 — Naprawić semantykę ack i zgodność historii predykcji

Priorytet P0. Kroki 21–22. Właściciele: server-runtime i client-runtime. Zależność: E046-05.

Potwierdzone: `runtime/admission/actions/movement.rs::admit_motion` podnosi `resolved_input_seq` do numeru odrzuconej próbki. Wcześniejsze próbki mogą nadal znajdować się w `pending_movement_inputs`. Klient w `prediction.rs::discard_resolved_inputs` usuwa wszystkie próbki <=ack. Taki ack nie dowodzi rozstrzygnięcia całego prefiksu.

Gdzie dodatkowo: `crates/server-runtime/src/runtime/inputs.rs`, snapshoty, `crates/client-runtime/src/session/`, `apps/client/src/renderer/camera.rs`, `crates/client-runtime/src/prediction.rs`.

Do wykonania:

- Ustalić ack jako bezpieczną granicę usuwania historii: nie potwierdzać przez nierozstrzygnięte wcześniejsze przyjęte próbki albo rozstrzygnąć je jawnie przed przesunięciem ack. Obsłużyć odrzucenie zbyt przyszłej próbki, Busy, zastąpienie próbki tego samego ticka i spóźnienie.
- Powiązać historię z każdą rzeczywiście przewidzianą próbką/tickiem. Obecny klient przewiduje fixed-step, ale wysyła według keepalive/zmiany kierunku; wykazać odtwarzalność kroków pomiędzy wysłaniami.
- Replay rozpoczyna `held_input` od default; sprawdzić odtwarzanie utrzymywanego kierunku od snapshotu do pierwszej oczekującej próbki. Odtwarzać dokładnie zgodną politykę i kolizję.
- Prześledzić granice 2/8 ticków, 50 ms, 128 próbek, reset przy transferze/teleporcie/braku kolizji i pojedynczy impuls skoku. Przy overflow usunąć także zależny stan czasu/predykcji w kontrolowany sposób.

Akceptacja źródłowa: ręczne ślady stanów dla odrzucenia wysokiej sekwencji z wcześniejszą oczekującą, Busy, dwóch próbek jednego ticka, braku pakietu i transferu. Ack nigdy nie usuwa wejścia nadal możliwego do wykonania. Nie pisać ani nie uruchamiać nowych testów w tej fazie; przykłady służą review kodu.

## E046-07 — Domknąć trwałe encje i rzeczywistą rezerwację zapisu

Priorytet P0. Kroki 23–26. Właściciele: world, storage, server-runtime. Zależność: E046-06.

Stan: krok 26 pozostaje In progress w dzienniku; nie wystarcza obecność `initialize_new_content` i `apply_gameplay_effects`.

Gdzie: `crates/world/src/entity/`, `crates/storage/src/{budget,format,loaded_state}.rs`, `format/`, `player_store/`, `persistence/`, `crates/server-runtime/src/world_registry/entity_state.rs`, `storage.rs`, `startup.rs`, `lifecycle.rs`.

Do wykonania: prześledzić spawn→upsert→remove→despawn→autosave→load dla encji, w tym bez komponentów. Udowodnić przywrócenie next ID i content_initialized, schematy przed load, brak ponownego dosiewania portalu. Wspólne limity: 2048 B, 16 komponentów, 16384 trwałych encji, 4 MiB payloadów, snapshot 128 MiB. Każda mutacja rezerwuje pełny narzut przez ten sam encoder przed zmianą; rezerwacja profilu obejmuje maksimum. Sprawdzić zbiorcze efekty, usunięcia i odtworzenie budżetu. Zapis 10 ma deterministyczną kolejność, transakcyjne load i writer strumieniowy; wersje 2–9 kończą start błędem bez nadpisania. Snapshot asynchroniczny jest niezmienny i ma rewizję; scalanie nie gubi końcowego zapisu.

Akceptacja: tabela producentów snapshotu obejmuje autosave i shutdown; wszystkie mutacje używają rezerwacji, odrzucony batch nie zmienia świata/budżetu. Usunięte stare component-object wrappery i martwy rehydrate. Krok 26 zamknąć dopiero po dowodach także z E046-09/10.

## E046-08 — Uzupełnić kontrakt systemów gameplay i lifecycle świata

Priorytet P1. Kroki 27–29. Właściciele: gameplay, server-runtime, sample. Zależność: E046-07.

Potwierdzone: `runtime/schedule.rs::ModuleSystemContext` nie zawiera numeru ticka; `run_stage` go nie przyjmuje. `startup_ran` jest pojedynczym bool dla harmonogramu. Wymagany jest tick i Startup związany z właściwym cyklem świata.

Gdzie: `crates/gameplay/src`, `crates/server-runtime/src/{modules,composition}.rs`, `runtime/{schedule,tick}.rs`, wykonanie efektów w world_registry, `games/sample/src/portal.rs`, `scenario.rs`, interakcje obu klientów.

Do wykonania: dodać tick do neutralnego read-only kontekstu i przekazać z owner lane. Zdefiniować lifecycle Startup dla nowych/odtworzonych światów i późniejszego dodania świata; nie powtarzać dosiewania. Potwierdzić before/after, stabilny tie-break ID, odrzucanie cyklu/duplikatów/brakujących zależności, rejestr per instancja zamrożony po starcie. Efekty <=256/1 MiB, walidacja całego batcha i budżetu przed commit po etapie. Portal sample ma komponent, waliduje rodzaj/dystans, wysyła efekt transferu; runtime waliduje context/readiness/placement. Usunąć WorldDefinition.portal, UsePortal i równoległy stan portalu.

Akceptacja: zewnętrzny moduł ma użyteczny tick i ograniczone efekty bez infrastruktury. Istniejący portal jest konsumentem publicznego API, zapisu i replikacji. Startup nie dubluje encji po restarcie.

## E046-09 — Dokończyć audyt transakcji i ograniczeń replikacji

Priorytet P0. Kroki 30–32. Właściciele: protocol, server-runtime, client-runtime. Zależności: E046-07/08.

Gdzie: `crates/protocol/src`, `crates/server-runtime/src/replication.rs`, stan połączenia, `crates/world/src/entity/`, `crates/client-runtime/src/{replicas,session/replication}.rs`, `apps/client/src/renderer/session/` i terminalowe interakcje.

Do wykonania: prześledzić begin/record/end/ack dla full i delta, context, sync ID i base/target revision. Sprawdzić długości przed alokacją, 2048 B komponent/4096 B wire, nieznany schemat, server-only wiadomości. Udowodnić niezależne ack klientów i zachowanie dziennika 4 MiB; fallback pełnego obrazu po wypadnięciu z dziennika. Interest spatial przez aktywne sektory, Global jawny, limit 256 według dystansu i ID z raportowaniem osiągnięcia limitu. Budżet 64 KiB/tick liczy rzeczywiste wire bytes i zachowuje priorytet sterowania; sync wygasa po 30 s. Zweryfikować niezmienność obrazu pełnej synchronizacji podczas mutacji. Klient staging ogranicza encje/komponenty, waliduje całą deltę przed commit, ack dopiero po commit, transfer czyści stan. Sprawdzić encję pustą i usunięcie ostatniego replikowanego komponentu.

Akceptacja: źródłowe ślady dwóch klientów z różnymi ack, zmiany interest, przerwania full i błędu w końcu delta. Nie ma połowicznego publikowania ani globalnego kasowania zmian po pierwszym kliencie.

## E046-10 — Potwierdzić shutdown z rzeczywistym wynikiem zapisu

Priorytet P0. Krok 33. Właściciele: platform, aplikacje, runtime’y, storage, telemetry. Zależności: E046-04/07/09.

Gdzie: `apps/server/src/main.rs`, `crates/platform/src`, `crates/server-runtime/src/lifecycle.rs`, `crates/storage/src/persistence/`, bootstrap client-runtime, `apps/client/src/renderer/controller.rs`.

Zakres: zmapować sygnał ctrlc/termination, admin stop i zamknięcie właściciela do jednego ShutdownRequest. Udowodnić kolejność stop admission→freeze mutations→stop jobs→capture→save→release endpoint→logs. Całość 30 s, logi do 2 s w tym budżecie. Przejrzeć każdy Drop/send/recv/join: brak bezterminowych operacji, jawna lista niezakończonych workerów, timeout zapisu nie jest sukcesem ani dowodem anulowania I/O. Klient zatrzymuje wyłącznie OwnedServer.

Akceptacja: wynik końcowego zapisu dociera do exit status; wszystkie workery ujęte w tabeli lifecycle; poprzedni atomowy snapshot chroniony. Zachowanie sygnałów pozostaje do odrębnych scenariuszy runtime.

## E046-11 — Zamknąć pokrycie PL/EN, fontu i wejścia

Priorytet P1. Kroki 34–37. Właściciele: localization i aplikacja klienta. Zależność: E046-10.

Gdzie: `crates/localization/`, `apps/client/src/{messages,input,config,options}.rs`, `console/{glyph_atlas,text,theme}.rs`, obsługa IME w `renderer/window.rs`, `renderer/window_events.rs`, terminal i katalogi FTL.

Stan: fontdue/Fluent/grafemy są obecne. Atlas ma jeden rozmiar 20 px, statyczny OnceLock i zestaw zakresów znaków; klucz zawiera size_bits, ale to samo nie dowodzi poprawnego DPI ani poprawnego układu grafemów.

Zakres: sprawdzić pełne zdania UI w obu frontendach i zmianę języka istniejącej historii; fallback PL→EN→klucz z diagnostyką, składnia komend/JSON niezależna. Oddzielić techniczne przyczyny od UI. Zweryfikować metryki, baseline, zastępczy glif, obcięcie komórek atlasu, łączenie znaków i DPI; nie dodawać RTL/CJK. Potwierdzić font i licencję w repo, nie tylko README. IME preedit nie wywołuje komend; ruch/kasowanie/zaznaczenie nie przecinają grafemu. InputMapper musi egzekwować modal>console>map>gameplay, odróżniać tekst od klawisza, walidować konflikty i czyścić held na utracie fokusu. Prześledzić zapis/odczyt bindings obu buildów.

Akceptacja: macierz komunikatów, kontekstów i operacji edycji ma źródłowe pokrycie; problemy poprawione u właściciela. Sam klucz cache z rozmiarem nie zamyka wymagania DPI. Ocena wizualna odroczona.

## E046-12 — Dokończyć SettingsStore i rzeczywiste wydzielenie aplikacji

Priorytet P1. Kroki 38–39. Właściciel: apps/client. Zależność: E046-11.

Potwierdzone w `apps/client/src/application.rs`: SettingsStore przechowuje stringowe startup/requested/overrides/sources, lecz nie ma efektywnej wartości/powodu różnicy. ClientApplication odwołuje się do `crate::renderer::client_game::ClientGameState`. WorldTransitionController zawiera HashMap i bool-e zamiast jawnej maszyny stanów.

Gdzie dodatkowo: `renderer.rs`, `renderer/{settings,quality,client_game,controller,session}.rs`, `renderer/session/`, `client_commands/`, `terminal_client/`, `config.rs`.

Do wykonania: przenieść stan i zachowanie sesji/świata poza moduł renderer. Wprowadzić kontrolowane przejścia przygotowanie→gotowość→commit/anulowanie/błąd i usunąć kombinacje flag dopuszczające sprzeczności. Renderer dostaje widok prezentacji. Ustalenie effective/requested/source/reason musi być wspólne dla get/set/reset/show/save, locale i bindings; runtime/server settings nie stają się preferencjami GPU. Zweryfikować reset do rzeczywistej warstwy startowej. Wspólne frontendowe komendy nie mogą wymagać GPU. Nie przenosić całego Renderer do nowego pliku pod inną nazwą.

Akceptacja: operacje connect/transfer/set można wykonać przez właściciela aplikacji bez dostępu do GpuResources; terminal kompiluje się bez desktop; każdy stan przejściowy ma jedną legalną ścieżkę zakończenia; wartości efektywne mają jawny powód różnicy.

## E046-13 — Zakończyć rozdział przygotowania CPU i publikacji GPU

Priorytet P1. Krok 40. Właściciele: apps/client i renderer. Zależność: E046-12.

Gdzie: `apps/client/src/renderer/{shadow_prepare,shadow_map_cache,scene_bridge,render,gpu}.rs`, `renderer.rs`, `crates/renderer/src/{world_geometry_gpu,ui_pipeline,render_frame}.rs`.

Stan: ShadowState i ShadowUniform przeniesiono do Renderer; RenderSceneSnapshot przekazuje referencje, a upload uniformu wykonuje render. Nadal `shadow_prepare` wywołuje mutujący cache GPU, a `shadow_map_cache::update_shadow_map_vertices` materializuje geometrię CPU wewnątrz impl GpuResources. Nie jest to gotowy rozdział.

Do wykonania: CPU przygotowuje niezmienny rezultat geometrii/selekcji/uniformu bez właściciela zasobów GPU; GPU wykonuje upload/release/encode. Zdefiniować wynik uploadu i warunki publikacji całego zestawu, a nie częściowych map/buforów. Stan niegotowy unieważnia cień. Zawęzić widoczność uchwytów i mutacji GPU; zachować CPU-only RenderFrame i build bez desktop. Usunąć importy pozostawione po przenosinach (m.in. EnvironmentFrame/EnvironmentFrameStatus i Zeroable w shadow_gpu.rs). Uzgodnić pomiar shadow_prepare_cpu z tym, co rzeczywiście mierzy.

Akceptacja: render nie zna ConnectionController, sesji, storage ani konfiguracji procesu; przygotowanie CPU komunikuje się wynikami, nowy aktywny zestaw jest kompletny, release świata ma kontrolowany punkt. Przegląd wszystkich producentów i odbiorców, nie tylko RenderSceneSnapshot.

## E046-14 — Powiązać castery i odbiorców z tą samą publikacją geometrii

Priorytet P0. Krok 41. Właściciele: aplikacja klienta i renderer. Zależność: E046-13.

Gdzie: `renderer/{gpu,shadow_prepare,shadow_map_cache,diagnostics,sector_scene}.rs`, `crates/renderer/src/world_geometry_gpu.rs`, jego `prepared_upload.rs`, render/scene encoder.

Potwierdzone: cache mapy identyfikuje instance/fingerprint/center/radius; brak w kluczu rewizji opublikowanej geometrii i generacji origin. WorldGeometryGpu ma wersję generacji/treści świata, co nie zastępuje identyfikatora konkretnej publikacji zestawu LOD.

Do wykonania: jeden identyfikator publikacji scene geometry, wspólny dla odbiorców i casterów; klucz cache zawiera world/fingerprint/geometry revision/origin generation. Rebase aktualizuje obie strony spójnie, wynik starej pracy nie publikuje się. LOD częściowo wgrany pozostaje niekompletny; zachować invalidation zamiast rysować mieszany zestaw. Rozszerzyć sky info o scene revision, origin generation, źródło, texel i przyczynę invalidation. Nie zmieniać rozdzielczości, liczby map/passów ani biasu; nie wprowadzać Low jako obejścia spójności.

Akceptacja: dla dowolnego encode można wskazać ten sam znacznik obu stron; przejście LOD/origin nie pozwala staremu cache udawać kompletnego. Wygląd i migotanie pozostają nieweryfikowane do osobnej fazy.

## E046-15 — Usunąć stare ścieżki i naprawić kontrolę architektury

Priorytet P1. Kroki 42–44. Właściciele: wszyscy zmienieni właściciele, skrypty. Zależności: E046-01–14.

Gdzie: `architecture.toml`, `scripts/check_architecture.py`, `check_static_refactor.py`, `check_file_lengths.py`, `scripts/wgsl_validator/`, Cargo manifests i zmienione źródła.

Potwierdzone: check_architecture buduje dict z deklaracji (gubi duplikaty), sprawdza opisane-nieistniejące pakiety, ale nie ma symetrycznej bramki dla członka workspace bez deklaracji. Skan źródeł obejmuje wybrane katalogi, nie wszystkie domeny. check_static_refactor nadal wymaga literalnych eksportów i starych sygnatur.

Do wykonania:

- Sprawdzić dokładnie jedną deklarację każdego członka workspace: duplikat, nieopisany i nieistniejący pakiet mają błąd.
- Produkcyjny graf uwzględnia normal/build/optional/platform; dev oddzielnie. Domeny i runtime’y nie zależą od sample także przechodnio. Serwer bez GPU, renderer bez okna/transportu/storage; objąć diagnostics/gameplay/localization.
- Skan źródeł obejmuje filesystem/proces/sieć/GPU we wszystkich domenach. Opisać ograniczenie regex-skanu, bez twierdzenia o dowodzie czystości.
- Usunąć wymagania historycznych eksportów; zostawić rzeczywiste layouty Rust/WGSL i inwarianty własności. Nie dublować kontroli zależności w dwóch skryptach.
- Przejrzeć legacy/migration/compat/dead_code, reexporty, stare zasoby i Cargo dependencies. Zachować nazwy opisujące odrzucany format. Testowe callsite’y dostosować mechanicznie, bez nowych scenariuszy/asercji.
- Zachować limit 500 fizycznych linii również dla untracked plików i shaderów; uaktualnić źródła/warianty walidatora WGSL.

Akceptacja: odczyt skryptów pokazuje kompletne reguły; brak obejść allow(unused/dead_code), pustych implementacji i wyłączania targetów. Skryptów jeszcze nie uruchamiać.

## E046-16 — Uzgodnić dokumentację ze stanem kodu

Priorytet P1. Krok 45. Zależności: E046-01–15.

Gdzie: `docs/architecture.md`, `architecture-execution.md`, `docs/protocol.md`, dokumentacja konfiguracji/zapisu/renderera i README zmienionych pakietów.

Zakres: opisać rzeczywistych właścicieli oraz przepływy principal→permissions, input→simulation→ack→replay i component→save→replication. Opisać logger, retencję i korelację runów; dodawanie komunikatu PL/EN, akcji wejścia i modułu na portalu. Wskazać wersje 18/10/5 i odrzucanie starych danych bez migracji/nadpisania. Usunąć opisy usuniętych funkcji. Developer provider nie jest systemem kont; publiczne auth i TLS poza zakresem. Każdy finding tego epicu ma zamykające issue i dowód. Zrewidować stare Complete, szczególnie 26/28/38/39/40.

Akceptacja: dokumentacja opisuje kod, nie intencję; osobno kod, kompilacja i runtime. Nie obiecywać wydajności/odporności/wyglądu bez pomiarów.

## E046-17 — Wykonać jedyną końcową fazę kompilacji i walidacji

Priorytet P0, bramka epicu. Krok 46. Zależności: wszystkie E046-01–16 zamknięte źródłowo.

Gdzie: cały workspace, Cargo.lock, skrypty statyczne i walidator WGSL.

Wykonać dokładnie w kolejności:

1. Zaktualizować Cargo.lock tylko dla potrzebnych zależności, bez zbiorczego upgrade.
2. Sformatować zmieniony Rust.
3. `rtk python scripts/check_file_lengths.py`
4. `rtk python scripts/check_architecture.py`
5. `rtk python scripts/check_static_refactor.py`
6. `rtk cargo build --workspace --all-targets --all-features --locked`
7. `rtk cargo build -p minimium-client --no-default-features --locked`
8. `rtk cargo run --manifest-path scripts/wgsl_validator/Cargo.toml --locked`
9. Przejrzeć końcowy diff i git status.

Każdy błąd naprawić u właściciela; po zmianie ponowić dotknięte kontrole. Nie wyłączać targetów/features, nie dodawać atrap ani szerokich wyciszeń. `--all-targets` kompiluje istniejące testy, ale ich nie uruchamia. Nie uruchamiać cargo test, gry, loadtestu ani benchmarków.

Akceptacja: wszystkie polecenia zakończone sukcesem na końcowym stanie drzewa, zapis rzeczywistych wyników i ograniczeń. Nie zastępować pełnego builda sprawdzeniem jednego crate.

## E046-18 — Zamknąć epic raportem i oddzielnym backlogiem uruchomieniowym

Priorytet P1. Krok 47. Zależność: E046-17.

Gdzie: `docs/architecture-execution.md`, ten epic i końcowy raport dla użytkownika.

Zakres: przypisać dowody do każdego kroku 01–47, wymienić nowe kontrakty i usunięte ścieżki, podać dokładne wyniki poleceń. Wprost zapisać „Testów i scenariuszy uruchomieniowych nie wykonywano”. Jeśli coś nie spełnia kryterium, pozostawić issue otwarte i nie zamykać epicu.

Osobny, niewykonywany w tym zakresie backlog: cienie przy nieruchomej kamerze i zmianach LOD/origin; transfer i portal/powrót; restart z zapisanymi i usuniętymi encjami; wolna sieć i wielu odbiorców replikacji; anulowanie zadań i presja cache; SIGTERM/shutdown oraz awarie zapisu; PL/EN, DPI, IME i grafemy; nadużycia wejść oraz rekonsyliacja. Każda taka sesja wymaga osobnego rozpoczęcia fazy testowej przez użytkownika.

Akceptacja: kompletna macierz 47 kroków i otwartych ryzyk; brak deklaracji potwierdzonego usunięcia migotania na podstawie kompilacji.
