# Georeferencja obserwatora

`minimium_core::sky` definiuje niezależny od renderera kontrakt obserwatora. Backend
astronomiczny dostaje `GeographicObserver` i `ObserverFrame`; nie odczytuje czasu
systemowego ani pozycji z kamery. Resolver wymaga `EnvironmentDescriptor` z warstwy
świata i odrzuca niezgodność profilu z topologią manifestu.

## Modele środowiska

| Profil i topologia | `SkyModel` | Wysokość |
| --- | --- | --- |
| `Interior` + `Dungeon` | `None` | Brak obserwatora nieba |
| `Earth` + `PlanetDefinition.mode = 1` | `EarthGeoreferencedV1` | GEBCO przed skalowaniem terenu |
| `EarthLike` + poprawna planeta proceduralna | `MappedEarthSkyV1` | Wysokość radialna świata gry |

Inne zestawienia profilu i topologii są błędem `EnvironmentProfileMismatch`. Starszy
manifest planety bez `PlanetDefinition` korzysta z powierzchni zwróconej przez
`planet_surface_for`; klient nie tworzy fikcyjnej definicji planety. `Earth` wymaga
definicji trybu 1 i już zainstalowanego pakietu GEBCO zgodnego z jej digestem.

## Pozycja, osie i jednostki

Wejściem jest `Position3Mm`, czyli dokładna pozycja stóp w milimetrach. Każda składowa
musi mieć wartość bezwzględną najwyżej 10 000 000 000 mm. Pozycja o promieniu nie większym
niż 1000 mm jest odrzucana. Standardowe oko ma wysokość 1700 mm; dopuszczalny zakres to
0–100 000 mm. Wartości `f32` z kamery nie są źródłem położenia astronomicznego.

Dla pozycji `p = [x, y, z]` i promienia `r = |p|`:

```text
latitude  = asin(clamp(y / r, -1, 1))
longitude = normalize(atan2(z, x), [-pi, pi))
```

`+Y` wskazuje północny biegun, `+X` równik na południku Greenwich, a `+Z` równik na
długości 90°E. Na dokładnym biegunie długość geograficzna wynosi umownie 0.

Lokalna baza świata gry:

```text
U = [ cos(phi) cos(lambda), sin(phi), cos(phi) sin(lambda) ]
N = [-sin(phi) cos(lambda), cos(phi), -sin(phi) sin(lambda) ]
E = [-sin(lambda), 0, cos(lambda)]
```

Spełnia ona `N × E = U`. Astronomy Engine `HOR` używa składowych `[north, west, up]`,
więc konwersja do świata gry ma postać `N*x - E*y + U*z`. Odbicie `reflectionX`
pozostaje wyłącznie w macierzy widoku Earth i nie jest powtarzane w długości geograficznej,
ramce obserwatora ani danych gwiazd.

`game_eye_world_m` opisuje wizualne położenie oka na pomniejszonej planecie. Jego promień
nie jest przekazywany do astronomii jako promień Ziemi. `game_planet_radius_m` jest
oddzielnym parametrem geometrii gry.

## Wysokość Earth

GEBCO zwraca surową interpolowaną wysokość `g` w metrach. Helper
`try_earth_elevation_m` nie skaluje ani nie ogranicza reliefu; wersja z oczekiwanym
digestem odrzuca brakujący lub niezgodny pakiet. Dotychczasowy sampler terenu korzysta
z tej samej interpolacji i zachowuje dotychczasowe przeliczenie wysokości.

Dla `s = height_scale_permille`, promienia planety gry `R`, radialnego położenia stóp
`r_mm`, wysokości oka `eye_height_mm`, poziomu morza `sea_level_mm` i limitu gór
`mountain_limit_mm`:

```text
scaled_gebco_mm   = round(g * s)
baseline_game_mm  = clamp(scaled_gebco_mm, sea_level_mm, mountain_limit_mm)
physical_anchor_m = max(g, sea_level_mm / s)
residual_eye_m    = (r_mm - R - baseline_game_mm + eye_height_mm) / 1000
physical_eye_m    = physical_anchor_m + residual_eye_m
```

Dzielenie `sea_level_mm / s` odbywa się w `f64`; ponieważ `s` jest skalą w promilach,
wynik wyraża metry fizyczne. Oko i skok zachowują skalę 1:1. Dno oceanu nie staje się
wysokością oka nad poziomem morza. Limit reliefu dotyczy geometrii gry, a nie surowej
wysokości fizycznego punktu odniesienia.

Przykład dla `g = 1000 m`, `s = 100`, bez detalu: stopy 100 m nad promieniem gry i oko
1,7 m dają wysokość 1001,7 m. Dla dna GEBCO `g = -4000 m`, morza na 0 mm i stóp na
powierzchni oceanu wynik wynosi 1,7 m.

Wysokość fizyczna poza zakresem −12 000–100 000 m jest błędem; nie przycinamy jej po
cichu. W V1 wartość GEBCO traktujemy jako odniesienie zbliżone do średniego poziomu
morza. Nie modelujemy geoidy, refrakcji zależnej od pogody ani lokalnej panoramy.

## Profil EarthLike i ograniczenia horyzontu

`MappedEarthSkyV1` używa ziemskiej szerokości i długości jako jawnej konwencji dla
proceduralnej planety. Wysokość oka wynosi `(r_mm - R + eye_height_mm) / 1000`, bez
GEBCO i bez dzielenia przez skalę reliefu. Nie opisuje to astronomii fikcyjnego układu
planetarnego.

Pozycje astronomiczne używają ziemskiego obserwatora, natomiast zasłanianie przez teren
i horyzont renderera pozostaje zgodne z pomniejszoną geometrią gry. Ten model nie obiecuje
zgodności czasu wschodu lub zachodu co do sekundy, szczególnie przy górach i niskim Słońcu.
Zmiana instancji, membership epoch lub manifestu unieważnia ramkę klienta; overview
korzysta z obserwatora gracza, a nie z położenia kamery globusa.

## Offline backend astronomiczny (E004-07)

`minimium-astronomy` jest zależnością klienta i korzysta z lokalnego, przypiętego
źródła Astronomy Engine C v2.1.19. Commit, URL-e i SHA-256 plików znajdują się
w `third_party/astronomy-engine/provenance.json`; licencja MIT jest dołączona.
Build wymaga kompilatora C, ale nie pobiera źródeł, nie uruchamia bindgen ani
libclang. `build.rs` definiuje `ASTRONOMY_ENGINE_NO_CURRENT_TIME`. Serwer i
`minimium-core` nie zależą od backendu.

Deklaracje `repr(C)` dla własnego mostka są generowane przez lokalny skrypt
wersji 1: `python scripts/generate_astronomy_bindings.py`. Wygenerowany plik ma
mniej niż 500 linii, dlatego nie wymaga wyjątku od limitu. Wyjątki polityki
rozmiaru dotyczą jedynie dwóch niezmienionych plików upstream.

Publiczne API używa POSIX UTC w milisekundach, radianów, metrów i wektorów f64.
Wejście UTC jest najpierw sprawdzane w zakresie core (2000–2100), a następnie
konwertowane całkowitoliczbowym odejmowaniem epoki J2000 i podzieleniem przez
86400000. `Astronomy_TimeFromDays` wyznacza TT swoim modelem delta-T; astronomia
nie odczytuje zegara systemowego i nie stosuje stref czasowych ani DST.

`SkyCalculator` jest właścicielem cache efemeryd i nie implementuje `Send` ani
`Sync`. Prywatny moduł FFI serializuje wszystkie cztery wejścia do C jednym
procesowym `Mutex<()>`, pobieranym dokładnie raz na wywołanie. To chroni również
niezależne kalkulatory utworzone w różnych wątkach przed wyścigiem globalnego
licznika `_CalcMoonCount` upstream. Zatruta blokada odzyskuje guard: nie chroni
stanu Rust ani transakcji, a C nie wykonuje callbacków/unwind do Rust.
Mostek sprawdza statusy,
wartości skończone oraz argumenty; Rust waliduje długości i ortonormalność
wektorów, macierzy, obserwatora oraz rekordów gwiazd. Błąd zwracany jest jako
`SkyError`, bez częściowego `SkyState`.

`SkyState.sun` i `SkyState.moon.body` zawierają kierunki topocentryczne bez
refrakcji (`geometric`) oraz po pojedynczej korekcie standardowej refrakcji
(`apparent`). „Geometryczny” nie odwraca upstreamowych poprawek czasu biegu
światła i aberracji. Księżyc jest topocentryczny, więc uwzględnia paralaksę;
faza wynika z wektorów do Słońca i obserwatora. `geocentric_sun_direction_world`
jest osobnym kierunkiem do oświetlenia globu i nie zależy od refrakcji lokalnego
obserwatora. Kierunki używają wspólnej macierzy EQJ→HOR→world z ramką #56.
Astronomy Engine przechowuje rotację jako `rot[input][output]`; bridge transponuje
ją do rustowego `[output][input]`. EQJ→HOR jest rotacją o wyznaczniku +1,
natomiast HOR=N/W/U → world ma wyznacznik −1 wynikający z konwencji osi gry.
Walidacja odróżnia te dwie transformacje; odbicie widoku Earth pozostaje w kamerze.

Promień Księżyca przyjęty przez minimium wynosi 1737,4 km jako jawny promień
średni modelu tarczy. Przypięty nagłówek upstream podaje promienie równikowy
1738,1 km i polarny 1736,0 km, a nie tę wartość średnią. Jasność V pochodzi z
upstreamowego modelu geocentrycznego; nie jest lux ani lokalną irradiancją.

Cache efemeryd ma dwa końce przedziału co 300000 ms czasu świata. Interpoluje
wektory EQJ, magnitudo liniowo, fazę ekliptyczną przez sinus i cosinus, a biegun
normalizuje po interpolacji. Lokalna rotacja, paralaksa i kierunki topocentryczne
są przeliczane dla bieżącego czasu i miejsca obserwatora. Skok czasu lub zmiana
rewizji/świata unieważnia cache; kod wylicza tylko aktualny przedział.

`update_star_epoch` przyjmuje do 500 ogólnych rekordów `StarAstrometry`, bez
zależności od formatu assetu. Uwzględnia ruch własny w przestrzeni EQJ od epoki
J2000 TT, zwraca osobny wynik z `star_epoch_revision` i buforuje go jako `Arc`.
Aktualizacja następuje przy zmianie doby, cofnięciu/skoku czasu, zmianie danych
lub jawnym unieważnieniu. W V1 gwiazdy nie mają paralaksy, aberracji ani
refrakcji; ich wygaszanie i zasłanianie horyzontem należy do renderera.

Dokładność backendu, referencje JPL/USNO i zachowanie GPU są częścią odroczonej
końcowej weryfikacji E004-16. Na etapie E004-07 nie uruchamiamy testów ani gry.
