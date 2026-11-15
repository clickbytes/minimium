# Library-level instructions

Applies to packages under `crates/` in addition to the repository-root `AGENTS.md`. Before changing a package, read its entry in `architecture.toml` and its own `README.md`. These are the authorities for existing dependency and API facts; this file adds only cross-package engineering rules.

## Ownership by role

- `engine-domain`: value models, state machines and deterministic algorithms should receive external data through explicit inputs/ports. Do not introduce filesystem, process, transport, GPU or UI effects into domain logic.
- `runtime`: coordinate domain owners, scheduling, bounded workers and adapters; keep authoritative owner-lane mutation explicit. A runtime may depend on adapters specified in `architecture.toml` but must not depend on sample-game composition.
- `adapter`: own declared effects (e.g. `asset-io`, `storage`, `transport`, `platform`, `astronomy` FFI); do not put game rules into I/O adapters. Do not impose a blanket I/O ban on these crates.
- `presentation`: `minimium-renderer` owns CPU frame preparation and GPU resources/shaders, not connection/session authority, player policy or world persistence.
- `gameplay`, `terrain`, `fluids`, `diagnostics`, `localization` are real domain crates in the current workspace; use their actual README contracts, not historical package lists.

## Public contracts and state

- Prefer a small owner API over public mutable fields. Validate parameters and invariants at the owner boundary; return typed errors instead of panicking on recoverable user input.
- A component is data, a system is behavior, a module registers capabilities, a resource owns an external lifetime, and an adapter performs effects. Do not force every object into an ECS.
- Use stable, typed identifiers for instances, sectors, revisions, components and world context. Separate authoritative, predicted and visual states. Every publication consumed across systems needs an explicit version/context.
- Make units, epochs and clocks visible in types or documentation (millimeters/meters, f64 world positions/f32 render offsets, real monotonic time versus world UTC).
- Keep bounded queues, caches, worker results and GPU memory: document budgets and rejection/fallback behavior. A `Result` should describe partial effects if an operation is not atomic.
- Before moving a public type or mutator, find all consumers (including desktop, terminal, local admin, save and optional feature builds). Move producers and consumers together and remove stale paths.

## Package documentation and verification

- Maintain each package README's existing sections: Purpose / Non-goals, Public API, Dependencies, Data flow, Ownership and execution, Invariants and errors, Commands, Change guide, Verification.
- Update `architecture.toml` only when the actual role or Cargo boundary changes. Avoid placeholder packages and fake future interfaces.
- Check the relevant package with `cargo check -p <actual-package> --locked` when Cargo is available; for dependency changes use `python scripts/check_architecture.py`. For hand-maintained code check `python scripts/check_file_lengths.py`.
- Tests and running the application follow the root `AGENTS.md` final-verification gate. Document unrun checks explicitly instead of claiming success.
