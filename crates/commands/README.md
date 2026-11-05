# minimium-commands

## Purpose / Non-goals

Owns only the generic static command metadata registry. Command values and validation belong to their feature domains; execution remains in host applications.

## Public API

`CommandRegistry<T, E>` and `CommandSpec<T, E>` provide longest-name/alias matching, parser dispatch and usage metadata.

## Dependencies

Has no project dependencies. It does not depend on platform adapters, runtime, protocol or renderer.

## Data flow

Feature owners register parsers and usage metadata. Hosts decide when to parse and how to handle typed results.

## Ownership and execution

This crate owns no mutable state, I/O or worker. Registration and execution remain with the host application.

## Invariants and errors

Preserve longest-name matching and stable alias resolution. Keep feature-specific ranges, units and error text in their owning domains.

## Commands

Exports a generic registry, not a centralized engine command vocabulary. Feature domains register their own specifications.

## Change guide

Change registry matching and metadata behavior here. Change command values and validation in their feature domain; change execution in the host.

## Verification

Run `rtk cargo check -p minimium-commands -p minimium-platform -p minimium-server -p minimiumctl -p minimium-client` plus architecture and file-length checks. Runtime acceptance is deferred.
