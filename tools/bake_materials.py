"""Build-time KTX2 cooker. Python + numpy; no client startup generation."""
import argparse
import hashlib
import importlib.util
from pathlib import Path
import struct
import tempfile
import zlib
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
RECIPE = ROOT / "games/sample/materials/recipe.py"


def blocks(image):
    edge = image.shape[0]
    if edge < 4:
        image = np.pad(image, ((0, 4-edge), (0, 4-edge), (0, 0)), mode="edge")
        edge = 4
    return image.reshape(edge//4, 4, edge//4, 4, 4).transpose(0, 2, 1, 3, 4).reshape(-1, 16, 4)


def bc4(values):
    hi = values.max(1).astype(np.uint8)
    lo = values.min(1).astype(np.uint8)
    distance = hi.astype(np.float32) - lo
    t = np.clip(np.rint((hi[:, None].astype(np.float32) - values) * 7 / np.maximum(distance[:, None], 1)), 0, 7).astype(np.uint8)
    lookup = np.array([0, 2, 3, 4, 5, 6, 7, 1], np.uint8)
    indices = lookup[t].astype(np.uint64)
    packed = np.sum(indices << (np.arange(16, dtype=np.uint64) * 3), axis=1, dtype=np.uint64)
    result = np.empty((len(values), 8), np.uint8)
    result[:, 0], result[:, 1] = hi, lo
    result[:, 2:] = (packed[:, None] >> (np.arange(6, dtype=np.uint64) * 8)).astype(np.uint8)
    return result


def bc1(values):
    rgb = values[:, :, :3].astype(np.uint32)
    c565 = ((rgb[:, :, 0] >> 3) << 11) | ((rgb[:, :, 1] >> 2) << 5) | (rgb[:, :, 2] >> 3)
    # Fit along the largest component range instead of ordering packed RGB565 words.
    axis = np.ptp(rgb, axis=1).argmax(-1)
    channel = np.take_along_axis(rgb, axis[:, None, None], axis=2)[:, :, 0]
    row = np.arange(len(rgb))
    a, b = c565[row, channel.argmax(1)], c565[row, channel.argmin(1)]
    high, low = np.maximum(a, b), np.minimum(a, b)
    high = np.maximum(high, low + 1).clip(0, 65535)
    low = np.minimum(low, high - 1)
    def unpack(v):
        return np.stack(((v >> 11) * 255 / 31, ((v >> 5) & 63) * 255 / 63, (v & 31) * 255 / 31), -1)
    a, b = unpack(high), unpack(low)
    palette = np.stack((a, b, (a*2+b)/3, (a+b*2)/3), 1)
    # One block row at a time limits cooker peak memory.
    error = ((rgb[:, :, None, :] - palette[:, None, :, :]) ** 2).sum(-1)
    indices = error.argmin(-1).astype(np.uint32)
    packed = np.sum(indices << (np.arange(16, dtype=np.uint32)*2), axis=1, dtype=np.uint32)
    result = np.empty((len(values), 8), np.uint8)
    result[:, :2] = high.astype('<u2').view(np.uint8).reshape(-1, 2)
    result[:, 2:4] = low.astype('<u2').view(np.uint8).reshape(-1, 2)
    result[:, 4:] = packed.astype('<u4').view(np.uint8).reshape(-1, 4)
    return result


def encode(image, fmt):
    values = image if image.dtype == np.uint8 else np.rint(np.clip(image, 0, 1)*255).astype(np.uint8)
    if fmt in (37, 43):
        return values.tobytes()
    out = []
    for row in range(0, len(values), 64):
        block = blocks(values[row:row+64]) if len(values) <= 64 else None
        # blocks requires a square; reshape rectangular strips explicitly.
        if block is None:
            strip = values[row:row+64]
            block = strip.reshape(16, 4, len(values)//4, 4, 4).transpose(0, 2, 1, 3, 4).reshape(-1, 16, 4)
        if fmt == 132:
            encoded = bc1(block)
        elif fmt == 141:
            encoded = np.concatenate((bc4(block[:, :, 0]), bc4(block[:, :, 1])), 1)
        else:
            encoded = np.concatenate((bc4(block[:, :, 3]), bc1(block)), 1)
        out.append(encoded.tobytes())
    return b''.join(out)


def descriptor(fmt):
    compressed = fmt not in (37, 43)
    model = {132: 128, 137: 130, 141: 132}.get(fmt, 1)
    samples = {132: [(0, 63, 0)], 137: [(0, 63, 15), (64, 63, 0)],
               141: [(0, 63, 0), (64, 63, 1)]}.get(fmt, [(i*8, 7, c) for i, c in enumerate((0, 1, 2, 15))])
    body = struct.pack('<IHH4B4B8B', 0, 2, 24+16*len(samples), model, 1,
                       2 if fmt in (43, 132) else 1, 0,
                       3 if compressed else 0, 3 if compressed else 0, 0, 0,
                       (8 if fmt == 132 else 16) if compressed else 4, *([0]*7))
    for offset, length, channel in samples:
        body += struct.pack('<HBBIII', offset, length, channel, 0, 0, 0xffffffff if compressed else 255)
    return struct.pack('<I', len(body)+4) + body


def reduce(image, srgb=False, normal=False):
    image = image.astype(np.float32)/255 if image.dtype == np.uint8 else image
    edge = len(image)//2
    if srgb:
        rgb = image[:, :, :3]
        linear = np.where(rgb <= .04045, rgb/12.92, ((rgb+.055)/1.055)**2.4)
        linear = linear.reshape(edge, 2, edge, 2, 3).mean((1, 3))
        rgb = np.where(linear <= .0031308, linear*12.92, 1.055*linear**(1/2.4)-.055)
        return np.concatenate((rgb, np.ones((edge, edge, 1), np.float32)), -1)
    mip = image.reshape(edge, 2, edge, 2, 4).mean((1, 3))
    if normal:
        vector = mip[:, :, :3]*2-1
        vector /= np.maximum(np.linalg.norm(vector, axis=-1, keepdims=True), 1e-8)
        mip[:, :, :3] = vector*.5+.5
    return mip


def png(path, pixels):
    pixels = np.rint(np.clip(pixels, 0, 1)*255).astype(np.uint8)
    def chunk(kind, data):
        return struct.pack('>I', len(data))+kind+data+struct.pack('>I', zlib.crc32(kind+data))
    raw = b''.join(b'\0'+row.tobytes() for row in pixels[:, :, :3])
    header = struct.pack('>IIBBBBB', pixels.shape[1], pixels.shape[0], 8, 2, 0, 0, 0)
    path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR', header)+chunk(b'IDAT', zlib.compress(raw))+chunk(b'IEND', b''))


def previews(output, name, color, normal):
    directory = output/'previews'
    directory.mkdir(exist_ok=True)
    thumb = color
    while len(thumb) > 512:
        thumb = reduce(thumb, srgb=True)
    png(directory/f'{name}-albedo.png', thumb)
    start = len(color)//3
    color = color[start:start+512, start:start+512, :3]
    normal = normal[start:start+512, start:start+512, :3]*2-1
    light = np.array([.4, -.5, .76], np.float32)
    light /= np.linalg.norm(light)
    lighting = .3+.7*np.maximum((normal*light).sum(-1), 0)
    linear = np.where(color <= .04045, color/12.92, ((color+.055)/1.055)**2.4)
    linear *= lighting[:, :, None]
    png(directory/f'{name}-detail.png', np.where(linear <= .0031308, linear*12.92, 1.055*linear**(1/2.4)-.055))


def write_ktx(path, edge, fmt, layers):
    levels = edge.bit_length()
    dfd = descriptor(fmt)
    dfd_offset = 80 + levels*24
    cursor = (dfd_offset + len(dfd) + 7) & ~7
    entries = []
    payloads = []
    images = layers
    for level in range(levels):
        payload = b''.join(encode(image, fmt) for image in images)
        entries.append((cursor, len(payload), len(payload)))
        payloads.append((cursor, payload))
        cursor = (cursor + len(payload)+7) & ~7
        if level+1 < levels:
            images = [reduce(image, srgb=fmt in (43, 132), normal=fmt == 141 or path.name.startswith('normal')) for image in images]
    with path.open('wb') as out:
        # KTX2 requires physical mip payloads from smallest to largest.
        cursor = (dfd_offset + len(dfd) + 7) & ~7
        for level in reversed(range(levels)):
            payload = payloads[level][1]
            entries[level] = (cursor, len(payload), len(payload))
            payloads[level] = (cursor, payload)
            cursor = (cursor + len(payload)+7) & ~7
        out.write(b'\xabKTX 20\xbb\r\n\x1a\n')
        out.write(struct.pack('<13I2Q', fmt, 1, edge, edge, 0, 6, 1, levels, 0,
                              dfd_offset, len(dfd), 0, 0, 0, 0))
        for entry in entries:
            out.write(struct.pack('<3Q', *entry))
        out.write(dfd)
        for offset, payload in reversed(payloads):
            out.seek(offset)
            out.write(payload)


def bake(output):
    spec = importlib.util.spec_from_file_location('recipe', RECIPE)
    recipe = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(recipe)
    output.mkdir(parents=True, exist_ok=True)
    sources = Path(__file__).read_bytes()+b''.join(path.read_bytes() for path in sorted(RECIPE.parent.glob('*.py')))
    revision = hashlib.sha256(sources+np.__version__.encode()).hexdigest()
    stamp = output / 'recipe.sha256'
    if stamp.exists() and stamp.read_text() == revision and (output/'manifest.ron').exists():
        return
    variants = []
    # Generate each role once. Temporary byte maps avoid keeping six float arrays live.
    with tempfile.TemporaryDirectory(prefix='.cook-', dir=output) as temporary:
        temporary = Path(temporary)
        for role, name in enumerate(recipe.ROLES):
            print(f'Baking {name}', flush=True)
            maps = recipe.maps(role, 4096)
            previews(output, name, maps[0], maps[1])
            for index, image in enumerate(maps):
                np.save(temporary/f'{role}-{index}.npy', np.rint(np.clip(image, 0, 1)*255).astype(np.uint8))
            del maps, image
        for edge, formats in ((4096, (132, 141, 137)), (2048, (43, 37, 37))):
            files = []
            for index, (name, fmt) in enumerate(zip(('albedo', 'normal', 'packed'), formats)):
                path = output/f'{name}-{edge}.ktx2'
                layers = [np.load(temporary/f'{role}-{index}.npy', mmap_mode='r') for role in range(6)]
                if edge == 2048:
                    layers = [reduce(image, srgb=index == 0, normal=index == 1) for image in layers]
                write_ktx(path, edge, fmt, layers)
                del layers
                data = path.read_bytes()
                files.append(f'(name:"{path.name}",bytes:{len(data)},sha256:"{hashlib.sha256(data).hexdigest()}")')
            variants.append(f'(edge:{edge},compressed:{str(edge == 4096).lower()},maps:('+','.join(files)+'))')
    roles = ','.join('"'+r+'"' for r in recipe.ROLES)
    scales = ','.join(str(s) for s in recipe.SCALES)
    (output/'manifest.ron').write_text(f'(version:1,roles:({roles}),meters_per_tile:({scales}),height_blend:0.2,variants:('+','.join(variants)+'))')
    stamp.write_text(revision)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    bake(parser.parse_args().output)
