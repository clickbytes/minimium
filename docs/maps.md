# Map format v1 and v2

Minimium maps use a bounded UTF-8 text format. Both versions support axis-aligned boxes. Version 2 adds flat-color triangles that can be visual-only or used as walkable/ceiling collision surfaces. The client renders both; boxes and solid triangles are shared by the server and local client movement controllers.

```text
MINIMIUM_MAP 2
spawn 0 0 0
box 0 -0.15 0 20 0.3 20 0.26 0.29 0.32 solid
box 2 1 -1 1.2 1.3 1.2 0.75 0.18 0.12 visual
triangle 0 1 0 1 1 0 0.5 2 0 0.8 0.3 0.1
triangle -4 0.05 -2 -4 0.05 0 -2 0.75 0 0.48 0.34 0.2 solid
```

Each non-comment record has one of these forms:

- `MINIMIUM_MAP 1` must be the first non-empty record.
- `MINIMIUM_MAP 2` enables triangle geometry and must be the first non-empty record.
- `spawn X Y Z` sets the initial player foot position and must occur exactly once.
- `box CX CY CZ SX SY SZ R G B solid|visual` defines a centered axis-aligned box, its display color, and whether it blocks server movement.
- `triangle AX AY AZ BX BY BZ CX CY CZ R G B` defines one visual-only triangle with a flat color. Omitting the final mode keeps existing v2 maps visual-only.
- `triangle AX AY AZ BX BY BZ CX CY CZ R G B solid|visual` explicitly sets the triangle mode. Solid triangles support landing, ceilings, and shallow ramps; their absolute vertical normal component must be at least half of the normal length (at most 60 degrees from horizontal). The player controller samples the center and four points around its feet. Vertical triangle walls and arbitrary mesh collision are not supported.
- `#` begins a comment through the end of the line.

The parser rejects unsupported versions, unknown or malformed records, duplicate or missing spawn points, spawn points intersecting solid geometry, non-finite values, non-positive box dimensions, degenerate triangles, steep solid triangles, colors outside `0..=1`, coordinates outside `-1,000,000..=1,000,000`, maps larger than 1 MiB, more than 4096 boxes, or more than 8192 triangles. The format still has no rotated box collision, vertical triangle-wall or arbitrary mesh collision, textured materials, entities, zones, or streaming metadata. Server movement can step onto a solid box or follow a solid triangle surface within 0.55 world units of its current foot height. The sample map includes both box stairs and a shallow triangle ramp.

The default map is embedded in the client and server binaries. `minimium-client --map PATH` loads a custom map for rendering and passes it to a local server it starts. `minimium-server run --map PATH` loads the same format directly. The protocol welcome message carries a canonical map fingerprint; clients disconnect if it does not match their loaded map. The fingerprint is FNV-1a 64 over the spawn and ordered box records. Triangle records, when present, are appended with a distinct marker and their ordered vertices, color, and collision mode. Maps without triangles retain their v1 fingerprint across the format version change. Floats are IEEE-754 `f32` little-endian; negative zero is normalized to positive zero. This detects accidental map mismatch but is not a security feature.
