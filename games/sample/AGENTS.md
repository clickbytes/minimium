# Sample game content instructions

Applies to `games/sample/` in addition to the repository-root `AGENTS.md`. Read `games/sample/README.md` and `architecture.toml` before editing content. The sample is a game **consumer** of engine APIs, not a location for shared engine mechanisms.

## Content ownership

- This package owns sample world definitions, demo maps, deterministic seeds/presets, planet/dungeon arrangement, spawn and portal behavior, registered sample components/systems, combat/tool/impact balance, climate scenarios and authored materials.
- Keep stable IDs, hashes, seed derivation, component schema/encoding, generator revisions and world footprints unless a separate behavior/compatibility issue explicitly approves the change.
- Reusable generation, world/geometry, physics, fluid, command and rendering algorithms belong to their declared engine owners. Do not put special cases for the sample into those packages or make engine/runtime crates import `minimium-game-sample`.
- Sample systems register through the neutral `minimium-gameplay` contracts; `apps/server` composes initial entities and modules and `apps/client` registers the matching replicated component schemas. Do not reach into server runtime, transport or storage from sample systems.
- Render models/material recipes may provide sample-specific appearance; they must not become an alternative authoritative collision model, terrain revision or clock.

## Before editing

1. Identify whether the change is content/preset/balance or a reusable algorithm. If reusable, move the contract to the appropriate owner in a separate, explicit migration.
2. Trace the producer and every consumer (server composition, client bootstrap, protocol compatibility, asset cooking and persistence) before changing IDs or formats.
3. Make reproducibility and versioning explicit, including previously persisted worlds, rejected older formats and required manual acceptance.
4. Do not change sample content as a side effect of an agent-policy refactor. Use static checks when relevant and defer tests/visual acceptance under root `AGENTS.md`.
