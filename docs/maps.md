# Map format v1 and v2

Minimium maps use a bounded UTF-8 text format. Both versions support axis-aligned boxes. Version 2 adds flat-color triangles and static OBJ meshes that can be visual-only or solid collision geometry. Imported OBJ faces are expanded into the same triangle list used by the renderer and by server and local-client collision.

```text
MINIMIUM_MAP 2
spawn 0 0 0
box 0 -0.15 0 20 0.3 20 0.26 0.29 0.32 solid
box 2 1 -1 1.2 1.3 1.2 0.75 0.18 0.12 visual
triangle 0 1 0 1 1 0 0.5 2 0 0.8 0.3 0.1
triangle -4 0.05 -2 -4 0.05 0 -2 0.75 0 0.48 0.34 0.2 solid
triangle 6.5 0 -2 8.5 0 -2 7.5 2.5 -2 0.18 0.52 0.62 solid
mesh models/pyramid.obj 4 0 0 1 1 1 0.82 0.48 0.16 solid
```

Each non-comment record has one of these forms:

- `MINIMIUM_MAP 1` must be the first non-empty record.
- `MINIMIUM_MAP 2` enables triangle geometry and must be the first non-empty record.
- `spawn X Y Z` sets the initial player foot position and must occur exactly once.
- `box CX CY CZ SX SY SZ R G B solid|visual` defines a centered axis-aligned box, its display color, and whether it blocks server movement.
- `triangle AX AY AZ BX BY BZ CX CY CZ R G B` defines one visual-only triangle with a flat color. Omitting the final mode keeps existing v2 maps visual-only.
- `triangle AX AY AZ BX BY BZ CX CY CZ R G B solid|visual` explicitly sets the triangle mode. Solid triangles support landing, ceilings, shallow ramps, and wall collision. Walkable surfaces have an absolute vertical normal component of at least half the normal length (at most 60 degrees from horizontal); the player controller samples the center and four points around its feet. Steeper triangles block the player's axis-aligned collision box using triangle-box separating-axis checks.
- `mesh PATH TX TY TZ SX SY SZ R G B solid|visual` loads a Wavefront OBJ file relative to the map file, scales its vertices by positive per-axis values, then translates them. The record supplies one flat color and one collision mode for the whole mesh. Use a file-backed map load (`--map PATH`); the embedded/in-memory map parser has no filesystem base directory.
- OBJ support reads `v` positions and `f` faces, including positive or negative position indices and `v/vt/vn` face tokens. Concave planar polygons are triangulated while preserving winding. Texture coordinates, normals, materials, groups, and object names are ignored. Rotation, textures, and per-face materials are not supported. See [`games/sample/maps/mesh_demo.map`](../games/sample/maps/mesh_demo.map) and [`games/sample/maps/models/pyramid.obj`](../games/sample/maps/models/pyramid.obj).
- `#` begins a comment through the end of the line.

The parser rejects unsupported versions, unknown or malformed records, duplicate or missing spawn points, spawn points intersecting solid geometry, non-finite values, non-positive box dimensions, degenerate triangles, colors outside `0..=1`, coordinates outside `-1,000,000..=1,000,000`, maps larger than 1 MiB, more than 4096 boxes, or more than 8192 triangles. A map can reference at most 64 OBJ assets; each file is limited to 8 MiB, all mesh files together to 16 MiB, and each asset to 262144 positions. Faces are limited to 128 vertices and must be planar and non-degenerate. Mesh paths must be relative, cannot contain `.` or `..` path components, and are checked after canonicalization so symlinks cannot escape the map directory. The format still has no rotated box collision, textured materials, entities, zones, or streaming metadata. Server movement can step onto a solid box or follow a walkable triangle surface within 0.55 world units of its current foot height. The default sample map includes an elevated route with box stairs, a shallow triangle ramp, and a collidable vertical triangle outside the route.

The default map is embedded in the client and server binaries. `minimium-client --map PATH` loads a custom map for rendering and passes it to a local server it starts. `minimium-server run --map PATH` loads the same format directly. The protocol welcome message carries a canonical map fingerprint; clients disconnect if it does not match their loaded map. The fingerprint is FNV-1a 64 over the spawn and ordered box records. Triangle records, including triangles expanded from OBJ meshes, are appended with a distinct marker and their ordered vertices, color, and collision mode. Maps without triangles retain their v1 fingerprint across the format version change. Floats are IEEE-754 `f32` little-endian; negative zero is normalized to positive zero. This detects accidental map mismatch but is not a security feature.

Open the OBJ example with `cargo run -p minimium-client -- --map games/sample/maps/mesh_demo.map`. For a separately launched server and client, pass this same map path to both processes.


## Default demo route

games/sample/maps/demo.map uses these axis-aligned bounds (center plus half-size):

- Ground: X [-10, 10], Y [-0.3, 0], Z [-10, 10].
- Platform: X [-5, 5], Y [3.6, 4], Z [-8, -2].
- Bridge: X [-1, 1], Y [3.6, 4], Z [-2, 2].
- Deck: X [-2, 2], Y [3.6, 4], Z [2, 4].
- Step n (1 through 8): X [n-10, n-9], Y [0, 0.5n], Z [4, 6].

The ground meets the first step at Y 0. Adjacent steps meet at their X boundaries and rise by 0.5 each. Step 8 reaches Y 4 and meets the deck over X [-2, -1]; the deck meets the bridge at Z 2, and the bridge meets the platform at Z -2. The platform, bridge, and deck meet edge-to-edge at Y 4; step 8 meets the deck at Y 4, and the eight steps descend in 0.5 increments to the ground. The bridge underside is at Y 3.6, leaving 3.6 units of clearance above the ground. The ramp and triangle examples lie at X greater than 5, beyond the elevated route and staircase. This layout changes the default map fingerprint; clients and server must load the same map to pass the fingerprint check.
