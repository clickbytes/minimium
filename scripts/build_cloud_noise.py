"""Deterministic periodic 3D cloud texture (generated asset, no network).

R: gradient Perlin, G: inverted Worley distance, B: finer inverted Worley.
Sampling/mip filtering is done by the GPU. No runtime Python dependency.
"""
from pathlib import Path
import math

SIZE = 64
ROOT = Path(__file__).resolve().parents[1]


def hash3(x, y, z):
    h = ((x * 0x8da6b343) ^ (y * 0xd8163841) ^ (z * 0xcb1ab31f)) & 0xffffffff
    h = ((h ^ (h >> 16)) * 0x7feb352d) & 0xffffffff
    h = ((h ^ (h >> 15)) * 0x846ca68b) & 0xffffffff
    return (h ^ (h >> 16)) / 4294967296


def feature(x, y, z, period):
    x, y, z = x % period, y % period, z % period
    return (hash3(x, y, z), hash3(x + 191, y, z), hash3(x, y + 173, z))


def worley(p, period):
    c = [math.floor(v) for v in p]
    best = 4.0
    for z in range(c[2] - 1, c[2] + 2):
        for y in range(c[1] - 1, c[1] + 2):
            for x in range(c[0] - 1, c[0] + 2):
                f = FEATURES[period][z % period][y % period][x % period]
                d = (x + f[0] - p[0]) ** 2 + (y + f[1] - p[1]) ** 2 + (z + f[2] - p[2]) ** 2
                best = min(best, d)
    return 1.0 - min(1.0, math.sqrt(best))


def perlin(p, period):
    c = [math.floor(v) for v in p]
    f = [p[i] - c[i] for i in range(3)]
    w = [t * t * t * (t * (t * 6 - 15) + 10) for t in f]
    value = 0.0
    for z in range(2):
        for y in range(2):
            for x in range(2):
                g = FEATURES[period][(c[2] + z) % period][(c[1] + y) % period][(c[0] + x) % period]
                g = [v * 2 - 1 for v in g]
                length = math.sqrt(sum(v * v for v in g)) or 1.0
                dot = sum(g[i] / length * (f[i] - n) for i, n in enumerate((x, y, z)))
                weight = math.prod(w[i] if n else 1 - w[i] for i, n in enumerate((x, y, z)))
                value += dot * weight
    return max(0.0, min(1.0, 0.5 + value * 0.8))


FEATURES = {n: [[[feature(x, y, z, n) for x in range(n)] for y in range(n)] for z in range(n)] for n in (4, 8)}


def main():
    data = bytearray()
    for z in range(SIZE):
        for y in range(SIZE):
            for x in range(SIZE):
                p = [(v + 0.5) / SIZE * 4 for v in (x, y, z)]
                channels = (perlin(p, 4), worley(p, 4), worley([v * 2 for v in p], 8), 1)
                data.extend(round(v * 255) for v in channels)
    dest = ROOT / "assets/weather/cloud_noise.rgba"
    dest.write_bytes(data)
    print(f"Generated {SIZE}^3 RGBA cloud noise: {len(data)} bytes")


if __name__ == "__main__":
    main()
