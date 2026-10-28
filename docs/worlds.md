# World prototype: cube-sphere surface and a dungeon

The current experimental map has two **kinds of environment**:
- **PlanetSurface**: a cube mapped to a spherical visual mesh (12 subdivisions per face in the sample). The controller treats the sphere as an analytic surface, with gravity and camera-up directed toward the planet's centre, movement in its tangent plane and radial jumps.
- **Dungeon**: the existing planar box/triangle environment with constant down-Y gravity, walls, ceilings and floor collisions.

Both regions are currently in **one authoritative World and one coordinate system**, with a dungeon separated spatially from the planet. This is a deliberately small playable slice, **not** a persistent, independently instanced dungeon, a distributed world system, or a streaming planet. The server resolves portals after the movement phase; the client predicts the same transition and reconciles from authoritative snapshots. The wire protocol remains v8. Distance-based world-kind selection works for this disjoint sample but must be replaced with explicit realm/instance IDs when worlds overlap or instancing is introduced.

## Run

Build the workspace to ensure both separate processes use the same code:

```sh
cargo build --workspace
cargo run -p minimium-client -- --map games/sample/maps/planet_dungeon.map
```

WASD and mouse look work as before. Spawn is at the planet's north pole (0,0,0). Follow the orange arch roughly 8 units forward (-Z with the initial camera yaw) to enter the dungeon. Inside, walk toward +Z to find the blue exit arch. The player state (world-space position and health) continues to use existing snapshot v3, and the map fingerprint includes the procedural geometry and portal endpoints, so this custom map uses a different save path when run from the client.

## Map v3 additions

```text
MINIMIUM_MAP 3
spawn 0 0 0
planet 0 -48 0 48 12 0.27 0.59 0.39
portal 0 -0.67 -8 0 0 600 1.4
portal 0 0 607 0 0 4 1.4
```

- `planet CX CY CZ RADIUS SUBDIVISIONS R G B`: one cube-sphere per map, divisions 4..24 per face, radius 5..1000. Produces 12×divisions² visual-only triangles; an analytic radial sphere provides ground collision.
- `portal AX AY AZ BX BY BZ RADIUS`: feet-position proximity trigger at A that atomically teleports to B and clears motion. At most 32 portals; trigger radius 0.5..8 world units. Model two-way travel with two records whose destination points lie **outside** the other trigger, preventing bounce loops.
- `box`, `triangle` and `mesh` keep their v2 meanings; a map may omit planet/portals and continue to use planar physics.

## Known prototype limits

- The planet shell currently supports only the analytic surface. Arbitrary box/mesh collision on the planet, spherical combat hitboxes, orientation transport across the poles, terrain displacement, LOD, gravity blending and planetary streaming are **not yet implemented**.
- The dungeon has traditional Y-up collisions and no separate instance ID. World-kind is inferred from the planet's influence volume; a real MMO design needs explicit `WorldId/InstanceId` in the network state and persistence, a zone registry and a transition protocol with map/asset loading.
- Large planet radius with high subdivisions remains constrained by `MAX_MAP_TRIANGLES=8192` and current fully loaded vertex buffers. Do not claim 500-player/500 MHz performance on this prototype.
- This branch was authored through the GitHub connector, without Rust/Cargo execution in this environment. `cargo fmt/check` and in-game movement/transition still require verification; no tests were run here.
