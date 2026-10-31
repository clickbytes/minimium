"""Build the offline lake polygon asset from Natural Earth (public domain).

Only this explicit import command accesses the network. No Python packages needed.
The binary stores polygon rings, including holes, quantized to 1e-5 degree.
Generated geometry is exempt from the hand-maintained source line limit.
"""
import hashlib
import io
import json
from pathlib import Path
import struct
import urllib.request
import zipfile


ROOT = Path(__file__).resolve().parents[1]
DEST = ROOT / "assets" / "weather"
SOURCES = ["lakes", "lakes_europe", "lakes_north_america", "lakes_australia"]


def polygons(shp):
    offset = 100
    while offset < len(shp):
        _, words = struct.unpack_from(">2i", shp, offset)
        record = shp[offset + 8:offset + 8 + words * 2]
        offset += 8 + words * 2
        kind, = struct.unpack_from("<i", record)
        if kind == 0:
            continue
        if kind != 5:
            raise ValueError(f"Expected polygon, got shape {kind}")
        parts, count = struct.unpack_from("<2i", record, 36)
        starts = list(struct.unpack_from(f"<{parts}i", record, 44)) + [count]
        start = 44 + parts * 4
        points = list(struct.iter_unpack("<2d", record[start:start + count * 16]))
        rings = []
        for a, b in zip(starts, starts[1:]):
            ring = [(round(x * 100000), round(y * 100000)) for x, y in points[a:b]]
            if len(ring) >= 4:
                rings.append(ring)
        yield rings


def main():
    records, provenance = [], []
    for source in SOURCES:
        url = f"https://naciscdn.org/naturalearth/10m/physical/ne_10m_{source}.zip"
        with urllib.request.urlopen(url, timeout=90) as response:
            archive = response.read()
        provenance.append({"url": url, "sha256": hashlib.sha256(archive).hexdigest()})
        with zipfile.ZipFile(io.BytesIO(archive)) as z:
            records.extend(polygons(z.read(f"ne_10m_{source}.shp")))
    data = bytearray(b"MMLAKE01" + struct.pack("<I", len(records)))
    for rings in records:
        data.extend(struct.pack("<I", len(rings)))
        for ring in rings:
            data.extend(struct.pack("<I", len(ring)))
            for point in ring:
                data.extend(struct.pack("<2i", *point))
    DEST.mkdir(parents=True, exist_ok=True)
    (DEST / "lakes.bin").write_bytes(data)
    metadata = {
        "source": "Natural Earth 1:10 million lakes and regional supplements",
        "license": "Public domain",
        "terms": "https://www.naturalearthdata.com/about/terms-of-use/",
        "scope": "Generalized cartography; excludes many small ponds and rivers.",
        "format": "MMLAKE01, LE u32 polygon count; ring count; point count; i32 lon,lat / 100000",
        "sha256": hashlib.sha256(data).hexdigest(),
        "polygons": len(records),
        "sources": provenance,
    }
    (DEST / "lakes.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    print(f"Imported {len(records)} polygons, {len(data)} bytes")


if __name__ == "__main__":
    main()
