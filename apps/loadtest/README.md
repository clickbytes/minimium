# minimium-loadtest

## Purpose / Non-goals

Runs headless network clients against configured load scenarios. It does not render, host authoritative worlds or claim performance acceptance by existing alone.

The CLI options are declared in `src/config.rs` and parsed by the shared `minimium-commands` rules. Loadtest keeps its own scenario and thread ranges and remains a one-shot tool with no config file or live properties.

## Public API

The executable is configured through its CLI and scenario inputs; implementation is internal to the application.

## Dependencies

Uses protocol, core identifiers, platform local support and telemetry. It has no renderer dependency.

## Data flow

Scenario definitions drive client connections and traffic; collected counters and reports describe the run.

## Ownership and execution

The loadtest process owns scenario and client lifetimes.

## Invariants and errors

Client setup sends `Presentation(None)` before the `dev.profile` authentication
declaration; this provider is for development only. Load
clients do not receive material offers, download textures or depend on a GPU.

Scenarios must remain reproducible and report connection or protocol failures explicitly.

## Commands

No engine gameplay command declarations.

## Change guide

Change scenario behavior here; keep wire encoding in protocol and avoid interpreting load scaffolding as measured acceptance.

## Verification

Run `rtk cargo check -p minimium-loadtest` and `rtk python scripts/check_architecture.py`. Do not start load runs during implementation.
