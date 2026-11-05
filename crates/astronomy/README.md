# minimium-astronomy

## Purpose / Non-goals

Provides the existing client-side astronomical calculation backend, including its FFI adapter. It does not own world time, network sessions or GPU rendering.

## Public API

`SkyCalculator`, sky model values, errors and observed bodies are exported from `src/lib.rs`.

## Dependencies

Consumes environment and world contracts and is used by the graphical client. Server, ctl and core must remain independent of this adapter.

## Data flow

The client supplies time, observer and catalog context; the calculator returns celestial observations for presentation.

## Ownership and execution

Calculator/cache instances are owned by the client presentation side; FFI remains encapsulated here.

## Invariants and errors

Keep the current astronomy algorithms, model revision and coordinate conventions stable.

## Commands

None.

## Change guide

Change astronomical calculations here. Keep camera, GPU uniforms and UI in renderer code.

## Verification

Run `rtk cargo check -p minimium-astronomy` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.
