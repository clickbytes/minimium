# Clear-air sky and globe

The former weather implementation was removed on 2026-11-01. Its replacement is
the client-side [regional weather and cube cloud model](weather.md). Rain/snow,
fog volumes, lightning, thunder, weather surface effects and forecast requests
remain outside that first stage. The server has no weather worker, and the
workspace no longer includes `minimium-weather`. The current game protocol is
v18 and the current world snapshot format is v10; this renderer change does not
alter either contract.

World UTC, clock synchronization, day/night lighting, the Sun, Moon, stars,
celestial labels, object shadows and geographic terrain remain independent.
The Earth globe retains clear-air Rayleigh/Mie/ozone scattering and a small
artistic night fill for map readability. The surface sky continues to use its
existing clear-sky lighting profile.

`renderer/atmosphere_gpu` owns a fixed Earth optical profile, a camera-independent
128 × 64 RGBA16F optical-depth table and an 80-byte uniform. The table occupies
65,536 bytes and is constructed once with the GPU resources. Globe shading and
the separate limb pass use it; `sky quality` selects four or eight view samples.
Window resizing only rebuilds the atmosphere's scene-depth bind group. There are
no weather resources inside this atmosphere module. Cloud accumulation targets
belong to the separate `renderer/clouds` module.
These byte counts describe allocations, not measured performance.

The 3,368 Natural Earth lake polygons remain geographic data for globe coloring
in `assets/earth/lakes.bin`, read by shared `environment/lakes.rs`. Their provenance,
archive hashes and binary hash are in `assets/earth/lakes.json`. They are
generalized 1:10 million cartography and omit many small ponds and rivers.
The data are public domain; the recorded source and terms are in the metadata.
`scripts/import_earth_lakes.py` explicitly regenerates the asset using network
downloads. Ordinary builds and the game do not invoke it. The binary is generated
data, exempt from the source line limit; its importer remains subject to the limit.

Static verification covers workspace compilation, formatting, Clippy, composed
WGSL validation, CPU/GPU layouts and stage visibility, architecture and file-size
checks. Game execution and visual verification remain deferred under `AGENTS.md`.
