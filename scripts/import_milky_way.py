#!/usr/bin/env python3
"""Convert the pinned NASA SVS J2000 Milky Way EXR to linear RGB9E5."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

SOURCE_URL = "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004851/milkyway_2020_4k.exr"
SOURCE_SHA256 = "2eb802d6e68d170b410f766c7fec07f7518619f6b6708fdc81e9302d93e74fdb"
SOURCE_WIDTH = 4096
SOURCE_HEIGHT = 2048
OUTPUT_WIDTH = 2048
OUTPUT_HEIGHT = 1024
ASSET_PATH = Path("crates/assets/data/sky/milky-way-v1.rgb9e5")
MANIFEST_PATH = Path("crates/assets/data/sky/milky-way-v1.manifest.json")


def convert(source: Path) -> bytes:
    try:
        import numpy as np
        import OpenEXR
    except ImportError as error:
        raise SystemExit("install numpy and OpenEXR to regenerate this asset") from error

    raw = source.read_bytes()
    if hashlib.sha256(raw).hexdigest() != SOURCE_SHA256:
        raise SystemExit("NASA SVS source SHA-256 does not match the pinned file")
    image = OpenEXR.InputFile(str(source))
    window = image.header()["dataWindow"]
    width = window.max.x - window.min.x + 1
    height = window.max.y - window.min.y + 1
    if (width, height) != (SOURCE_WIDTH, SOURCE_HEIGHT):
        raise SystemExit("NASA SVS source dimensions do not match")
    pixels = np.stack(
        [np.frombuffer(image.channel(channel), dtype="<f2") for channel in "RGB"], axis=-1
    ).astype(np.float32)
    pixels = pixels.reshape(SOURCE_HEIGHT, SOURCE_WIDTH, 3)
    if not np.isfinite(pixels).all() or (pixels < 0).any():
        raise SystemExit("NASA SVS source contains non-finite or negative radiance")
    # Average in linear light; the source is a linear half-float EXR.
    pixels = pixels.reshape(OUTPUT_HEIGHT, 2, OUTPUT_WIDTH, 2, 3).mean(axis=(1, 3))
    maximum = pixels.max(axis=2)
    exponent = np.ceil(np.log2(np.maximum(maximum, 2.0**-24) / 511.0) + 24.0)
    exponent = np.clip(exponent, 0, 31).astype(np.uint32)
    quantum = np.exp2(exponent.astype(np.float32) - 24.0)
    mantissa = np.rint(pixels / quantum[..., None]).clip(0, 511).astype(np.uint32)
    packed = (
        mantissa[..., 0]
        | (mantissa[..., 1] << 9)
        | (mantissa[..., 2] << 18)
        | (exponent << 27)
    )
    return packed.astype("<u4").tobytes()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path, help="repository root")
    args = parser.parse_args()
    asset = convert(args.input)
    if len(asset) != OUTPUT_WIDTH * OUTPUT_HEIGHT * 4:
        raise SystemExit("generated RGB9E5 asset has an invalid length")
    digest = hashlib.sha256(asset).hexdigest()
    manifest = {
        "asset": ASSET_PATH.as_posix(),
        "asset_bytes": len(asset),
        "asset_sha256": digest,
        "format": "rgb9e5ufloat",
        "height": OUTPUT_HEIGHT,
        "source_sha256": SOURCE_SHA256,
        "source_url": SOURCE_URL,
        "width": OUTPUT_WIDTH,
    }
    asset_path = args.output / ASSET_PATH
    manifest_path = args.output / MANIFEST_PATH
    asset_path.parent.mkdir(parents=True, exist_ok=True)
    asset_path.write_bytes(asset)
    manifest_path.write_text(
        json.dumps(manifest, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8"
    )
    print(f"asset_bytes={len(asset)} asset_sha256={digest}")


if __name__ == "__main__":
    main()
