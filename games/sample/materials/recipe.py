"""Deterministic offline material recipes, without baked lighting or global waves."""
import sys
from pathlib import Path
import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from fields import cellular, elements, fbm, noise, smooth

ROLES = ("sand", "soil", "grass", "litter", "rock", "snow")
SCALES = (2.0, 2.0, 3.0, 3.0, 4.0, 2.0)
COLORS = ((.70, .61, .42), (.30, .22, .14), (.28, .36, .14),
          (.27, .20, .10), (.43, .44, .42), (.91, .94, .96))
RELIEF_METRES = (.0018, .018, .012, .008, .045, .003)


def sand(edge, seed):
    grains, _ = cellular(edge, min(512, edge//2), seed)
    mineral = noise(edge, min(379, edge//2), seed+11)
    h = .35+.35*(1-np.minimum(grains, 1))+.14*fbm(edge, seed+29)
    color = np.array(COLORS[0], np.float32)*(1+(mineral[..., None]-.5)*.13)
    color += (fbm(edge, seed+61, (11, 37))[:, :, None]-.5)*.035
    return h, color, .82+(mineral-.5)*.12, np.full_like(h, .97)


def soil(edge, seed):
    clods, gaps = cellular(edge, 43, seed)
    fine = fbm(edge, seed+13, (89, 239, 509))
    # Disconnected granular clods, not a connected polygon paving pattern.
    clod = 1-smooth(.12, .65, clods+(fine-.5)*.25)
    _, stones, tint = elements(edge, seed+23, 220, (.003, .012), (.55, 1.0))
    h = .25+.20*clod+.18*fine+.10*noise(edge, 997, seed+47)+.16*stones
    pigment = fbm(edge, seed+91, (9, 31, 103))
    color = np.array(COLORS[1], np.float32)*(1+(pigment[:, :, None]-.5)*.36)
    color += stones[:, :, None]*(.06+tint[:, :, None]*.04)
    roughness = np.clip(.94-.15*stones+(fine-.5)*.06, .65, 1)
    return h, color, roughness, .90+.08*clod


def grass(edge, seed):
    tufts = smooth(.32, .64, fbm(edge, seed+1, (13, 37, 101)))
    blades, fibers, tint = elements(edge, seed+19, 4600, (.004, .019), (.07, .16))
    pigment = fbm(edge, seed+53, (17, 71, 211))
    cover = np.maximum(blades*.9, tufts*.7)
    earth = np.array([.25, .19, .11], np.float32)
    green = np.array([.23, .32, .10], np.float32)
    dry = np.array([.43, .36, .15], np.float32)
    leaf_color = green+(dry-green)*tint[:, :, None]*.65
    color = earth+(leaf_color-earth)*cover[:, :, None]
    color *= 1+(pigment[:, :, None]-.5)*.20
    h = .20+.20*tufts+.38*fibers+.08*noise(edge, 277, seed+71)
    return h, color, .88+(pigment-.5)*.10, .86+.12*cover


def litter(edge, seed):
    leaves, folds, tint = elements(edge, seed+3, 750, (.006, .026), (.35, .65), leaf=True)
    sticks, twigs, _ = elements(edge, seed+7, 450, (.007, .033), (.025, .065))
    earth = np.array([.19, .14, .08], np.float32)
    leaf_color = np.stack((.22+tint*.17, .14+tint*.13, .065+tint*.07), -1)
    color = earth+(leaf_color-earth)*leaves[:, :, None]
    color *= 1-sticks[:, :, None]*.3
    pigment = fbm(edge, seed+31, (19, 59, 173))
    color *= 1+(pigment[:, :, None]-.5)*.16
    h = .15+.48*folds+.24*twigs+.08*noise(edge, 241, seed+67)
    return h, color, .91+(pigment-.5)*.10, .80+.18*np.maximum(leaves, sticks)


def rock(edge, seed):
    weathering = fbm(edge, seed+13, (7, 23, 79, 251))
    warp = (weathering-.5)*2
    _, gap = cellular(edge, 9, seed+1, (warp, np.roll(warp, edge//3, axis=0)))
    cracks = (1-smooth(.006, .035, gap))*smooth(.32, .58, weathering)
    chips = fbm(edge, seed+29, (47, 149, 467))
    h = .36+.28*weathering+.22*chips-.20*cracks
    mineral = fbm(edge, seed+101, (17, 67, 257))
    color = np.array(COLORS[4], np.float32)*(1+(mineral[:, :, None]-.5)*.29)
    color[:, :, 0] += (weathering-.5)*.035
    roughness = np.clip(.77+(chips-.5)*.18+cracks*.09, .5, 1)
    return h, color, roughness, .97-.15*cracks


def snow(edge, seed):
    settled = fbm(edge, seed+5, (17, 43, 127))
    crystals = noise(edge, min(733, edge//2), seed+31)
    h = .30+.28*settled+.10*crystals
    pigment = fbm(edge, seed+79, (11, 47, 191))
    color = np.array(COLORS[5], np.float32)*(1+(pigment[:, :, None]-.5)*.035)
    return h, color, .77+(crystals-.5)*.14, np.full_like(h, .99)


RECIPES = (sand, soil, grass, litter, rock, snow)


def maps(role, edge):
    h, color, roughness, ao = RECIPES[role](edge, 91473+role*1009)
    h = np.clip(h, 0, 1).astype(np.float32)
    # Metre-based central differences keep normal strength independent of resolution.
    strength = RELIEF_METRES[role]*edge/(2*SCALES[role])
    dx = (np.roll(h, -1, 1)-np.roll(h, 1, 1))*strength
    dy = (np.roll(h, -1, 0)-np.roll(h, 1, 0))*strength
    normal = np.stack((-dx, -dy, np.ones_like(h)), -1)
    normal /= np.linalg.norm(normal, axis=-1, keepdims=True)
    alpha = np.ones((edge, edge, 1), np.float32)
    albedo = np.concatenate((np.clip(color, 0, 1), alpha), -1)
    normal = np.concatenate((normal*.5+.5, alpha), -1)
    packed = np.stack((np.clip(roughness, .1, 1), np.clip(ao, 0, 1), h, np.full_like(h, .04)), -1)
    return albedo, normal, packed
