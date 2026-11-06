"""Periodic noise and wrapped surface elements. Distances use tile units."""
import numpy as np


def noise(edge, cells, seed):
    rng = np.random.Generator(np.random.PCG64(seed))
    grid = rng.random((cells, cells), dtype=np.float32)
    coordinate = (np.arange(edge, dtype=np.float32)+.5)*(cells/edge)
    cell = np.floor(coordinate).astype(np.int32)
    fraction = coordinate-cell.astype(np.float32)
    t = fraction**3*(fraction*(fraction*6-15)+10)
    a, b = cell % cells, (cell+1) % cells
    lower = grid[a[:, None], a[None, :]]*(1-t[None, :])
    lower += grid[a[:, None], b[None, :]]*t[None, :]
    upper = grid[b[:, None], a[None, :]]*(1-t[None, :])
    upper += grid[b[:, None], b[None, :]]*t[None, :]
    return lower*(1-t[:, None])+upper*t[:, None]


def fbm(edge, seed, frequencies=(7, 19, 53, 139), persistence=.5):
    result = np.zeros((edge, edge), np.float32)
    total, weight = 0.0, 1.0
    for octave, cells in enumerate(frequencies):
        if cells > edge//2:
            break
        result += noise(edge, cells, seed+octave*7919)*weight
        total += weight
        weight *= persistence
    return result/max(total, 1e-8)


def cellular(edge, cells, seed, warp=None):
    """F1 and F2-F1, evaluated in row strips to bound temporary memory."""
    rng = np.random.Generator(np.random.PCG64(seed))
    features = .15+rng.random((cells, cells, 2), dtype=np.float32)*.7
    coordinate = (np.arange(edge, dtype=np.float32)+.5)*(cells/edge)
    first_out = np.empty((edge, edge), np.float32)
    gap_out = np.empty_like(first_out)
    for row in range(0, edge, 128):
        xx, yy = coordinate[None, :], coordinate[row:row+128, None]
        if warp is not None:
            xx = xx+warp[0][row:row+128]
            yy = yy+warp[1][row:row+128]
        ix, iy = np.floor(xx).astype(np.int32), np.floor(yy).astype(np.int32)
        fx, fy = xx-ix.astype(np.float32), yy-iy.astype(np.float32)
        first = np.full((len(iy), edge), np.inf, np.float32)
        second = first.copy()
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                point = features[(iy+dy) % cells, (ix+dx) % cells]
                distance = (point[:, :, 0]+dx-fx)**2
                distance += (point[:, :, 1]+dy-fy)**2
                second = np.minimum(second, np.maximum(first, distance))
                first = np.minimum(first, distance)
        first_out[row:row+128] = np.sqrt(first)
        gap_out[row:row+128] = np.sqrt(second)-np.sqrt(first)
    return first_out, gap_out


def elements(edge, seed, count, radius, aspect, leaf=False):
    """Wrapped overlapping pebbles, blades or leaves, with per-element variation."""
    rng = np.random.Generator(np.random.PCG64(seed))
    mask = np.zeros((edge, edge), np.float32)
    relief = np.zeros_like(mask)
    tint = np.zeros_like(mask)
    for _ in range(count):
        center = rng.uniform(0, edge, 2)
        length = rng.uniform(*radius)*edge
        width = length*rng.uniform(*aspect)
        angle = rng.uniform(0, np.pi*2)
        sine, cosine = np.sin(angle), np.cos(angle)
        extent = max(2, int(np.ceil(length+1)))
        xs = np.arange(-extent, extent+1, dtype=np.float32)
        xx, yy = xs[None, :]-center[0] % 1, xs[:, None]-center[1] % 1
        u = (xx*cosine+yy*sine)/max(length, 1)
        v = (-xx*sine+yy*cosine)/max(width, .7)
        distance = u*u+v*v
        if leaf:
            distance += .12*np.sin(u*17+rng.uniform(0, 6))*np.abs(v)
        coverage = np.clip((1-distance)*4, 0, 1)
        shape = np.sqrt(np.clip(1-distance, 0, 1))
        if leaf:
            shape *= .35+.65*np.exp(-np.abs(v)*18)
        indices = np.ix_((int(center[1])+xs.astype(int)) % edge,
                         (int(center[0])+xs.astype(int)) % edge)
        tint[indices] = np.where(coverage > mask[indices], rng.uniform(.1, .9), tint[indices])
        relief[indices] = np.maximum(relief[indices], shape*rng.uniform(.5, 1))
        mask[indices] = np.maximum(mask[indices], coverage)
    return mask, relief, tint


def smooth(a, b, value):
    t = np.clip((value-a)/(b-a), 0, 1)
    return t*t*(3-2*t)
