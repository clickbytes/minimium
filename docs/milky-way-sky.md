# Droga Mleczna na niebie

Renderer próbuje jedną teksturę nieba z danymi NASA SVS „Deep Star Maps 2020”.
Używa wersji `milkyway_2020_4k.exr` w układzie równikowym ICRF/J2000, bez
jasnych gwiazd Hipparcosa i Tycho. Gwiazdy punktowe nadal pochodzą z katalogu
HYG i są rysowane osobno. Źródło opisuje położenia, ciemne pasma pyłu i
nierównomierną jasność Drogi Mlecznej; nie jest absolutną kalibracją jasności
ekranu.

- Źródło: https://svs.gsfc.nasa.gov/4851/
- Bezpośredni plik: https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004851/milkyway_2020_4k.exr
- SHA-256 pliku: `2eb802d6e68d170b410f766c7fec07f7518619f6b6708fdc81e9302d93e74fdb`
- Kredyty wymagane przez NASA SVS: NASA/Goddard Space Flight Center Scientific
  Visualization Studio; Gaia DR2: ESA/Gaia/DPAC.
- NASA SVS udostępnia swoje materiały jako public domain, o ile przy danej
  wizualizacji nie podano wyjątku: https://svs.gsfc.nasa.gov/

`scripts/import_milky_way.py` weryfikuje hash lokalnego EXR, uśrednia liniowe
bloki 2×2 i zapisuje 2048×1024 piksele w `Rgb9e5Ufloat` bez gamma. Generowanie
wymaga lokalnie pakietów Python `OpenEXR` i `numpy`; nie są zależnościami gry.
Opublikowany asset wygenerowano z `OpenEXR 3.5.2` i `numpy 2.5.4`.
Przykład:

```powershell
python -m pip install OpenEXR numpy
python scripts/import_milky_way.py --input C:\path\milkyway_2020_4k.exr --output .
```

Manifest obok assetu zapisuje źródło, hash, rozmiar i format. Tekstura zajmuje
8 MiB na GPU. Shader odwraca macierz `eqj_to_world`, oblicza rektascensję i
deklinację kierunku piksela, a następnie próbuje mapę z RA=0 pośrodku i RA
rosnącą w lewo. Horyzont, atmosfera, zmierzch i Księżyc tłumią warstwę na
powierzchni. W Overview mapa jest tłem kosmicznym za globem i gwiazdami.

Jasność `0.02` na powierzchni i `0.03` w Overview to kalibracja prezentacji,
nie wynik pomiaru fotometrycznego. Barwy są częściowo odbarwione, by
przypominały widzenie nocne. Do późniejszej oceny wizualnej potrzebne są
obrazy przy nowiu i pełni, w głębokiej nocy i zmierzchu, z pasmem wysoko i
blisko horyzontu oraz po zmianie FOV.
