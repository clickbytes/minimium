"""Aggregate local ESA WorldCover 2021 v200 GeoTIFFs to two-minute fractions.

Requires rasterio and numpy. Reads supplied tiles only; untouched cells are nodata.
Output is presentation data and never participates in geometry fingerprints.
"""
import argparse
import hashlib
from pathlib import Path
import struct
import numpy as np
import rasterio
from rasterio.warp import reproject, Resampling
from rasterio.transform import from_origin
from rasterio.windows import from_bounds

ATTRIBUTION = ("© ESA WorldCover project 2021 / Contains modified Copernicus Sentinel "
               "data (2021) processed by ESA WorldCover consortium\n"
               "ESA WorldCover 10 m 2021 v200, Zanaga et al. (2022)\n"
               "https://doi.org/10.5281/zenodo.7254221\nCC BY 4.0\n")
# Unvegetated/built ground maps to soil; wetlands retain underlying vegetation.
CLASSES = {10: (0, 0, .15, .85, 0, 0), 20: (0, .2, .6, .2, 0, 0),
           30: (0, .1, .9, 0, 0, 0), 40: (0, .4, .6, 0, 0, 0),
           50: (0, .6, 0, 0, .4, 0), 60: (.25, .25, 0, 0, .5, 0),
           70: (0, 0, 0, 0, 0, 1), 80: (1, 0, 0, 0, 0, 0),
           90: (0, .3, .5, .2, 0, 0), 95: (0, .1, .1, .8, 0, 0),
           100: (0, .4, .6, 0, 0, 0)}


def sha256(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024*1024), b''):
            value.update(block)
    return value.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('tiles', nargs='+', type=Path)
    args = parser.parse_args()
    output = args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_suffix('.part')
    width, height = 10800, 5400
    with temporary.open('wb') as stream:
        stream.write(b'MMWC21\0\0'+struct.pack('<II', width, height))
        stream.truncate(16+width*height*7)
    grid = np.memmap(temporary, dtype=np.uint8, mode='r+', offset=16, shape=(height, width, 7))
    sources = []
    for path in sorted(args.tiles):
        with rasterio.open(path) as src:
            if src.crs != rasterio.crs.CRS.from_epsg(4326):
                raise ValueError(f'{path}: expected WorldCover WGS84')
            left, bottom, right, top = src.bounds
            col, row = round((left+180)*30), round((90-top)*30)
            cols, rows = round((right-left)*30), round((top-bottom)*30)
            if not (0 <= col < width and 0 <= row < height and cols > 0 and rows > 0
                    and col+cols <= width and row+rows <= height):
                raise ValueError('tile outside world grid')
            if grid[row:row+rows, col:col+cols, 6].any():
                raise ValueError('overlapping input tiles')
            # Process one output row at a time; a full 3-degree source tile can be huge.
            for strip in range(rows):
                strip_top = top-strip/30
                window = from_bounds(left, strip_top-1/30, right, strip_top,
                                     src.transform).round_offsets().round_lengths()
                classes = src.read(1, window=window)
                valid = np.isin(classes, tuple(CLASSES)).astype(np.float32)
                result = np.zeros((1, cols, 7), np.float32)
                transform = from_origin(left, strip_top, 1/30, 1/30)
                # Average class indicator fields to fractions, never nearest labels.
                for channel in range(7):
                    field = valid.copy() if channel == 6 else np.zeros_like(valid)
                    if channel < 6:
                        for label, weights in CLASSES.items():
                            if weights[channel]:
                                field[classes == label] = weights[channel]
                    reproject(field, result[:, :, channel], src_transform=src.window_transform(window),
                              src_crs=src.crs, dst_transform=transform, dst_crs=src.crs,
                              resampling=Resampling.average)
                result[:, :, :6] /= np.maximum(result[:, :, 6:], 1e-8)
                grid[row+strip:row+strip+1, col:col+cols] = np.rint(result*255).clip(0, 255).astype(np.uint8)
            sources.append(f'{path.name} {sha256(path)}')
    grid.flush()
    del grid
    temporary.replace(output)
    output.with_suffix('.attribution.txt').write_text(ATTRIBUTION+'\n'+'\n'.join(sources), encoding='utf-8')
    output.with_suffix('.sha256').write_text(sha256(output))


if __name__ == '__main__':
    main()
