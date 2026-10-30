# Katalog gwiazd HYG 4.1

`minimium-assets` dostarcza stały katalog 1000 gwiazd przez
`embedded_sky_catalog()` oraz lekką tożsamość
`embedded_sky_catalog_identity()`. Wybór jest globalny i nie zmienia się
z czasem, położeniem gracza ani widocznością. Renderer filtruje widoczne gwiazdy
później. Serwer może odczytać `catalogue_id` i SHA-256 bez dekodowania rekordów
lub zależności od crate’u astronomicznego.

## Źródło i odtwarzanie

Przypięte teksty licencji i deterministyczne produkty mają reguły `eol=lf`
w `.gitattributes`. Dzięki temu Windows checkout nie zmienia bajtów, do których
odnoszą się hashe i kontrola odtwarzalności. Ta sama reguła chroni vendor C i MIT
w backendzie astronomii; nie zmienia przypiętych źródeł ani manifestów hashy.

Importer korzysta wyłącznie z lokalnego wejścia; nie pobiera plików podczas
uruchamiania gry ani budowania crate’ów. Produkcyjny asset wygenerowano z:

- HYG Database v4.1, plik `hyg/CURRENT/hygdata_v41.csv`;
- commit `c7f7f883fe678cc7680169a50ccd7dcc49b060ce`;
- rozmiar `33 932 548` bajtów;
- Git blob SHA-1 `ba2dec4eb0f6768914c7fc1051258100214ddf84`;
- SHA-256 `d9f69fd86bbf90a4e4d52b4c5c53eacfa6dfc0bfdef85bfd94f095e0bebe4ebd`.

HYG opisuje `ra/dec` w epoce i równonocy 2000.0, `mag` jako pozorną wielkość
wizualną, `ci` jako B−V, `x/y/z` w parsekach oraz `vx/vy/vz` w parsekach na rok.
Wektory są już w układzie równikowym EQJ; importer nie obraca ich ani nie
normalizuje.

Utrzymujący pobiera dokładny URL z przypiętego commita do pliku tymczasowego,
a potem uruchamia importer z katalogu repozytorium jako `--output`:

```powershell
curl.exe --fail --location --output $env:TEMP\hygdata_v41.csv `
  https://raw.githubusercontent.com/astronexus/HYG-Database/c7f7f883fe678cc7680169a50ccd7dcc49b060ce/hyg/CURRENT/hygdata_v41.csv
python scripts/import_hyg_stars.py --input $env:TEMP\hygdata_v41.csv --output .
```

Importer używa standardowej biblioteki Pythona i sprawdza rozmiar, Git blob SHA-1
oraz SHA-256 przed parsowaniem. Wymaga kolumn o jawnych nazwach i bez duplikatów;
limity wejścia to 64 MiB, 200 000 rekordów i 16 KiB na rekord. CSV jest dekodowany
jako UTF-8, a liczby parsowane bez ustawień locale.

Wynik bieżącego importu: 119 626 wierszy źródła, 119 625 poprawnych kandydatów
po wykluczeniu Słońca, 0 odrzuconych kandydatów i dokładnie 1000 wybranych gwiazd.
Próg selekcji to katalogowa wartość `mag` 4.58. Cztery wybrane rekordy nie mają
`ci`; otrzymują neutralne 0.65 i są wymienione w manifeście. Wszystkie wybrane
rekordy mają kompletny wektor prędkości. Brak wymaganej wartości `mag`, `ra`,
`dec` lub współrzędnej kartezjańskiej odrzuca rekord i zapisuje jego HYG id.
Nieparsowalna, NaN/Inf lub przepełniona wartość, częściowo brakująca prędkość,
niepoprawne ID albo duplikat ID przerywa import bez publikacji nowych plików.

Wynikowy asset ma SHA-256
`8804e57b7e1e6740814a3177f37bcf0b5d1a384230aead56a3dfe035e3b1b09e`, payload
`a0c107236de7ffe485c53ff203a94714049a7823e324932182c20e262b33c26f`, a pełny
tekst CC BY-SA 4.0 ma SHA-256
`28a9529c7d0bb4dc51f4bf5c116a3d16ef247a052f7591466768ddf563fd1cf5`. Skróty
źródła i licencji HYG są zapisane w manifeście.

## Membership i rekord

Importer wybiera 1000 rekordów rosnąco po `(Decimal(mag), HYG id)`. Wyklucza
`id=0` (Słońce). Nie używa `absmag`, odległości, nazw ani warunków lokalnej
widoczności. Dokładny remis jest rozstrzygany ID. Późniejsza zmiana HYG lub
selekcji wymaga nowego `catalogue_id`/schematu, a nie automatycznej aktualizacji.

Pojedynczy HYG id jest punktem świetlnym. Nie scalmy dodatkowo składników układów
wielokrotnych po nazwie, HIP, HR ani bliskości; identyczne kierunki pozostają
osobnymi rekordami. Manifest raportuje powtarzające się niepuste HIP/HR i grupy
identycznych źródłowych `x/y/z`, bez zmiany membership. Brak wszystkich trzech
składowych prędkości daje `[0,0,0]` i wpis w `missing_proper_motion_ids`.
Częściowy brak jest błędem. `mag` i `ci` są danymi fotometrycznymi dla renderera;
mapowanie koloru należy do E004-11.

## Format `stars-v1.bin`

Plik ma dokładnie 64 096 bajtów: 96-bajtowy nagłówek i 1000 rekordów po 64 bajty.
Wszystkie pola wielobajtowe są little-endian; format nie używa `repr(C)` ani
paddingu Rust.

| Offset nagłówka | Pole |
| --- | --- |
| 0–7 | ASCII `MNSKY1K0` |
| 8–9 | schema `u16 = 1` |
| 10–11 | rozmiar rekordu `u16 = 64` |
| 12–15 | `catalogue_id = 2` (`u32`) |
| 16–19 | count `u32 = 1000` |
| 20–23 | flags `u32 = 0` |
| 24–31 | epoka `f64 = 2000.0` |
| 32–63 | SHA-256 źródłowego CSV |
| 64–95 | SHA-256 payloadu rekordów |

Każdy rekord zawiera `hyg_id:u32`, `hip_id:u32` (0 oznacza brak),
`apparent_v_mag:f32`, `color_index_bv:f32`, `position_eqj_pc:[f64;3]` i
`velocity_eqj_pc_per_jyear:[f64;3]`. Magnitudo i B−V są ponownie sprawdzane po
konwersji do f32. Asset SHA-256 obejmuje nagłówek i payload.

`star-labels-v1.tsv` zawiera 1000 nazw w tej samej kolejności co rekordy binarne.
Każdy wiersz ma HYG ID, tabulator i nazwę własną z HYG albo identyfikator
HIP/HR/HYG. Reader sprawdza zgodność ID. Manifest zapisuje SHA-256 tego pliku.

Plik `stars-v1.manifest.json` ma posortowane klucze, UTF-8, LF i końcowy newline.
Zawiera pin źródła i licencji, SHA-256, regułę selekcji, uporządkowane 1000 ID,
próg `Decimal`, liczniki/listy odrzuceń i fallbacków, powtarzające się HIP/HR,
grupy identycznych pozycji, wymiary formatu i hash assetu. Nie zawiera ścieżek
lokalnych, czasu generowania ani losowych identyfikatorów.

Wygenerowany `identity.generated.rs` zawiera `catalogue_id`, hash assetu i hash
źródła. Importer przygotowuje pliki w tymczasowym katalogu, weryfikuje ich
wzajemne skróty, następnie publikuje je z kopią poprzednich wersji na wypadek
błędu. Tożsamość jest publikowana na końcu; nie powstaje stan z nową tożsamością
i starym assetem.

## Reader i połączenie z astronomią

`embedded_sky_catalog()` używa `include_bytes!` i `OnceLock`; SHA-256 oraz dekodowanie
odbywają się najwyżej raz. Reader sprawdza długość, magic, schema, flagi, epokę,
catalogue id, hash źródła i payloadu, pełny digest, count, unikalne ID, porządek
niemalejącej magnitudo oraz skończone rekordy i niezerowe pozycje. Błąd jest
`CatalogError`; reader nie używa `unwrap` ani nie zastępuje złego pliku losowym
niebem.

Tożsamość jest niezależna od dekodowania i dostępna jako
`SkyCatalogIdentity { catalogue_id, sha256 }` w `minimium-assets`. Crate assets
nie zależy od astronomii; serwer nie wylicza efemeryd. Adapter klienta w
`renderer/environment/stars.rs` zachowuje źródłową kolejność i stabilne ID,
kopiuje pola EQJ do `StarAstrometry`, a fotometrię pozostawia przy rekordach.
Nie dodaje GPU, shaderów ani bytów świata.

## Pochodzenie i licencja

HYG Database v4.1 jest udostępniony na CC BY-SA 4.0. Dokładny 422-bajtowy plik
źródłowy `LICENSE` z przypiętego commita znajduje się jako
`HYG-LICENSE-NOTICE.txt`; importer sprawdza jego Git blob SHA-1 i SHA-256.
Dystrybucji assetu towarzyszy też pełny tekst prawny Creative Commons w
`LICENSE-CC-BY-SA-4.0.txt` oraz `ATTRIBUTION.md`, który podaje autora, pin i
dokonane zmiany. Dane i pochodny katalog są oznaczone CC BY-SA 4.0; kod silnika
zachowuje własną licencję.

Źródła: [README HYG 4.1](https://github.com/astronexus/HYG-Database/blob/c7f7f883fe678cc7680169a50ccd7dcc49b060ce/hyg/README.md),
[CSV HYG 4.1](https://github.com/astronexus/HYG-Database/blob/c7f7f883fe678cc7680169a50ccd7dcc49b060ce/hyg/CURRENT/hygdata_v41.csv),
[nota licencyjna HYG](https://github.com/astronexus/HYG-Database/blob/c7f7f883fe678cc7680169a50ccd7dcc49b060ce/LICENSE),
[tekst CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/legalcode.txt).

Testy odtwarzalności, granic selekcji i błędnych assetów należą do końcowej
weryfikacji E004-16 i nie były uruchamiane podczas implementacji.
