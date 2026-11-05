"""Stream the 60-arcsecond overview of the GEBCO 2026 ice-surface COG."""

import argparse
import hashlib
import json
from pathlib import Path
import urllib.request

import rasterio
from rasterio.windows import Window


SOURCE = "https://data.source.coop/ausantarctic/gebco/GEBCO_2026.tif"
WIDTH = 21_600
HEIGHT = 10_800
ROWS_PER_CHUNK = 128


def source_identity():
    request = urllib.request.Request(
        SOURCE, method="HEAD", headers={"User-Agent": "curl/8.0"}
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        etag = response.headers.get("ETag", "")
        version = response.headers.get("x-amz-version-id", "")
    if not etag or not version:
        raise RuntimeError("GEBCO mirror did not provide a source ETag and version")
    return {"url": SOURCE, "etag": etag, "version": version}


def ron_string(value):
    return json.dumps(value, ensure_ascii=True)


def write_index(path, identity, tile_digest):
    fields = [
        "version: 2",
        'edition: "GEBCO_2026_ice_surface_60arcsec"',
        f"source_url: {ron_string(identity['url'])}",
        f"source_etag: {ron_string(identity['etag'])}",
        f"source_version: {ron_string(identity['version'])}",
        f'tile_sha256: ["{tile_digest}"]',
    ]
    (path / "index.ron").write_text("(" + ", ".join(fields) + ")\n", encoding="utf-8")


def import_grid(package):
    staging = package.with_name(package.name + ".importing")
    if package.exists():
        raise RuntimeError(f"Package already exists: {package}")
    identity = source_identity()
    staging.mkdir(parents=True, exist_ok=True)
    identity_path = staging / "source.json"
    if identity_path.exists():
        previous = json.loads(identity_path.read_text(encoding="utf-8"))
        if previous != identity:
            raise RuntimeError("Source changed since the partial import; remove staging data")
    else:
        identity_path.write_text(json.dumps(identity, indent=2), encoding="utf-8")

    output = staging / "elevation.bil"
    existing = output.stat().st_size if output.exists() else 0
    row_bytes = WIDTH * 2
    if existing % row_bytes or existing > WIDTH * HEIGHT * 2:
        raise RuntimeError("Partial elevation.bil has an invalid size")
    start_row = existing // row_bytes
    with rasterio.Env(GDAL_DISABLE_READDIR_ON_OPEN="YES"):
        with rasterio.open("/vsicurl/" + SOURCE, overview_level=1) as grid:
            if (grid.width, grid.height, grid.count, grid.dtypes[0]) != (
                WIDTH,
                HEIGHT,
                1,
                "int16",
            ):
                raise RuntimeError("GEBCO overview geometry or type changed")
            with output.open("ab") as destination:
                for row in range(start_row, HEIGHT, ROWS_PER_CHUNK):
                    count = min(ROWS_PER_CHUNK, HEIGHT - row)
                    values = grid.read(1, window=Window(0, row, WIDTH, count))
                    destination.write(values.astype("<i2", copy=False).tobytes())
                    destination.flush()
                    print(f"GEBCO 1' import: {row + count}/{HEIGHT} rows", flush=True)

    if source_identity() != identity:
        raise RuntimeError("Source changed during import; partial data remain in staging")
    digest = hashlib.sha256()
    with output.open("rb") as data:
        for chunk in iter(lambda: data.read(16 * 1024 * 1024), b""):
            digest.update(chunk)
    write_index(staging, identity, digest.hexdigest())
    (staging / "ATTRIBUTION.txt").write_text(
        "GEBCO Bathymetric Compilation Group 2026 (2026). "
        "The GEBCO_2026 Grid - a continuous terrain model for oceans and land "
        "at 15 arc-second intervals. doi:10.5285/4f68d5c7-45eb-f999-e063-7086abc036fa\n"
        "This package contains the 1-arcminute overview of the ice-surface "
        "grid mirrored by the Australian Antarctic Division at Source Cooperative.\n",
        encoding="utf-8",
    )
    identity_path.unlink()
    staging.replace(package)
    print(f"Imported GEBCO 2026 1' package: {package}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package-dir", type=Path, default=Path("third_party/gebco-2026"))
    arguments = parser.parse_args()
    import_grid(arguments.package_dir.resolve())


if __name__ == "__main__":
    main()
