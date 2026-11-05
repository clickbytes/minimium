# minimium-client

## Purpose / Non-goals

Owns the desktop and text client application shells, input, console, renderer and command presentation. It consumes session, connection and clock synchronization contracts from `minimium-client-runtime`.

## Public API

Executable entry point is `src/main.rs`; internal modules provide startup and front-end behavior.

## Dependencies

Consumes renderer, client-runtime, core, environment, world, assets, asset-io, astronomy, platform, protocol, procgen, sectors, telemetry and sample-game content, plus GPU/window libraries. Headless server and ctl must not depend on this package.

## Data flow

Front ends collect input and poll sessions, then build presentation state and submit frames to the renderer. Text mode uses the session without GPU construction.

## Ownership and execution

The front-end loop owns UI and polls the runtime connection controller; the runtime owns its socket, bounded protocol state and local-server guard, while background connector work reports typed results.

## Invariants and errors

Preserve desktop/text command behavior and keep protocol/session handling out of rendering algorithms.

## Commands

Client command groups declare specs in `src/client_commands/`; matching and alias resolution use `minimium-commands::CommandRegistry`. Client action execution remains here.

## Change guide

Change window and platform startup here. Change protocol, connection, clock session, fixed-step prediction and snapshot reconciliation in client runtime. The runtime publishes a GPU-independent `ClientView`; the app maps its world, position, health and readiness values into `RenderFrame::client` alongside camera, environment and console data. The app owns frame scheduling, the surface handle/configuration, geography pixel preparation, visibility/caster selection and remaining GPU buffers. Renderer owns surface-image acquisition and presentation operations, camera/frame contracts, world/globe/UI vertex data and atlas layouts, sky/Sun/marker/world pipelines, main scene and shadow depth pass encoding, cloud and atmosphere pass encoding, shader sources, planet visibility/LOD, clear-air optics/LUT generation, visual quality profiles, shadow targets and GPU owner, star catalogue/pipeline, linear output encoder, globe overview texture/pipeline/LOD owner, overview geometry buffers, world geometry GPU catalog, UI resources, cloud frame uploads, Milky Way texture, shared frame bindings, celestial halo/Moon pipelines and final UI/output pass encoding.

## Verification

Run `rtk cargo check -p minimium-client` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.
