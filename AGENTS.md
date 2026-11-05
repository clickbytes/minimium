# Project workflow policy

## Tests

- Unit, integration, and end-to-end tests are deferred until the project's final verification phase.
- During normal implementation, do not add test suites or run tests. Treat testing as a project-wide final phase, not as a requirement for each feature or milestone.
- Begin that phase only when the user explicitly says the project is ready for final testing.
- Compilation, formatting, and static checks are separate from tests and may be used during implementation when useful.

## File size

- Hand-maintained code and shaders are limited to 500 physical lines per file. Do not evade the limit by combining lines.
- Every exception must document the specific reason and its scope. Generated files and data assets follow separately documented rules and must not be used to hide hand-maintained code.

## Domain ownership

- Read `architecture.toml` and the owning package's `README.md` before changing domain code.
- `docs/architecture.md` distinguishes current Cargo ownership from planned package boundaries; do not create empty placeholder crates.
- Keep domain code free of process, filesystem, transport, GPU and UI effects unless its declared package responsibility is an adapter or application.
- When moving behavior, keep the old consumer connected in the same change and update the ownership declaration and package README.
- Preserve existing world, protocol, save and generation behavior during structural changes.
