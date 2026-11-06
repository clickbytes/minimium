# minimium-commands

## Purpose / Non-goals

Owns generic command parsing, typed parse outcomes and errors, property metadata, and the format of immutable configuration layers. It contains no domain-specific command or property vocabulary. Domain state, value validation and execution remain with feature owners and host applications.

## Public API

`CommandRegistry<T, E>` and `CommandSpec<T, E>` provide longest-name/alias matching, atomic name registration, parser dispatch and usage metadata. `CommandOutcome` distinguishes an unknown name from a recognized command with invalid arguments. `PropertyRegistry` validates and lists static property descriptors without owning values. `PropertyRequest` parses `get`, `set`, `reset`, `list` and `help`; the requesting host checks access and calls the domain handler. `CommandError` carries a stable code, subject, reason and usage.

## Dependencies

Has no project dependencies. It does not depend on platform adapters, runtime, protocol or renderer.

## Data flow

Feature owners register parsers and property descriptors. Hosts parse requests and route typed results to the domain-owned validator and handler. Configuration parsing returns raw entries and source metadata; it does not mutate live domain state.

## Ownership and execution

This crate owns no live mutable domain state, I/O or worker. Its registries are immutable metadata after startup; file access and execution remain with adapters and host applications.

## Invariants and errors

Preserve longest-name matching and stable alias resolution. Keep feature-specific ranges, units and error text in their owning domains.

## Commands

Exports a generic registry, not a centralized engine command vocabulary. Feature domains register their own specifications.

## Change guide

Change registry matching and metadata behavior here. Change command values and validation in their feature domain; change execution in the host.

## Verification

Run `rtk cargo check -p minimium-commands -p minimium-platform -p minimium-server -p minimiumctl -p minimium-client` plus architecture and file-length checks. Runtime acceptance is deferred.
