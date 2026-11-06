# minimium-server-runtime

## Purpose / Non-goals

Owns the authoritative server lifecycle, world registry, tick, networking coordination, admission and persistence composition. It is headless and has no GPU or window role.

## Public API

`run(args)` parses server process arguments, initializes modules and startup state, then runs the authoritative runtime.

## Dependencies

Coordinates world, simulation, environment, protocol, transport, storage, platform, telemetry, assets, procgen and sample-game contracts. Astronomy and GPU crates are forbidden.

## Data flow

The process validates startup inputs, builds world instances, then applies bounded network inputs on the owner lane. Workers publish typed results back to the runtime; immutable save snapshots cross into storage.

## Ownership and execution

`ServerRuntime` owns mutable authoritative state on one lane. Worker threads do not mutate active worlds. Runtime shutdown joins workers and persists final state.

## Invariants and errors

Preserve current protocol/save compatibility, tick cadence, movement behavior, admission limits and shutdown ordering. Worker queue, protocol, storage and startup errors remain typed at their boundaries.

## Commands

Startup accepts `--material-package PATH`, `--material-http-port PORT` (0 selects
an available port), and `--climate-scenario PATH`. Both material variants are
validated before admission. The HTTP endpoint serves only manifest-listed files;
it owns one bounded I/O worker and joins it at shutdown. Open the selected HTTP
port alongside the game port for remote clients. Climate initializes and persists
in `world.climate.ron` under the world data lock, outside snapshots; subsequent
starts restore it and reject a replacement initialization argument. The required
material set is session-wide and retained through existing world prepare/ready.

Server-owned handlers compose shared command metadata. Game rules stay with their owning domain.

## Change guide

Change authoritative coordination and process lifecycle here. Reusable movement, collision, environment and storage logic belongs in their respective crates.

## Verification

Run `rtk cargo check -p minimium-server-runtime -p minimium-server` and architecture/file-length checks. Runtime acceptance is deferred.


Protocol v16 admits context-bound `SetAltitude` requests through the bounded
world-input queue. The authoritative handler requires fly mode, a planet, no
pending transfer, finite 0..10000 km and valid destination coordinates. It samples
local collision terrain, applies Earth relief scaling, preserves angular location
and movement settings, resets velocity and replies with the precise destination.
The existing player-store/snapshot path records the relocation; saves are unchanged.

Protocol v17 carries the immutable v5 peak-relief definition and independent
ClimateScenario.relief_influence. Startup --climate-relief-influence accepts a
finite 0..1, applies it to the loaded climate scenario, and atomically persists
the changed scenario under the data lock. Other scenario values are preserved.
It is valid for existing worlds without rebuilding terrain. Omission restores
the persisted setting; legacy climate files default to 0.

Malformed explicit climate-relief numbers report the option name and accepted
range. Omitted options are handled by bootstrap; the server never substitutes a
default for an explicitly supplied invalid number.
