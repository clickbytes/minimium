# minimium-server

## Purpose / Non-goals

Provides the server process entry point, selects optional allocation instrumentation, composes sample world and combat content, and forwards command-line arguments to `minimium-server-runtime`. It does not own runtime state or GPU presentation.

## Public API

The executable entry point is `src/main.rs`. `src/composition.rs` supplies the sample world builder and combat values through the runtime's neutral composition contract.

## Dependencies

Depends on `minimium-server-runtime`, `minimium-platform`, `minimium-game-sample` and the asset/world-generation contracts used by the sample builder. `minimium-telemetry` is optional for process allocation instrumentation. GPU, windowing and astronomy are forbidden.

## Data flow

The executable passes process arguments and a world/combat composition to the runtime. The builder runs during runtime initialization after world.seed discovery. The builder reads the saved planet definition, recovers a snapshot seed when needed and only then chooses or generates a seed, preserving the original error order. Generation precedes legacy migration and save loading.

## Ownership and execution

The runtime crate owns mutable authoritative state. This binary owns sample content selection, process startup and global allocator selection.

## Invariants and errors

Preserve the command-line interface and exit behavior.

## Commands

No domain commands are implemented in the binary.

## Change guide

Change sample world or combat assembly here. Change server lifecycle behavior in `minimium-server-runtime`.

## Verification

Run `rtk cargo check -p minimium-server` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.
