# minimiumctl

## Purpose / Non-goals

Provides a small administrative CLI that contacts a local server endpoint. It does not host the server or define gameplay rules.

## Public API

The executable entry point is `src/main.rs`; arguments select a data directory and one administrative command.

## Dependencies

Uses platform endpoint/admin adapters and current command parser plus core command values. It must remain independent of GPU and astronomy.

## Data flow

CLI text is parsed and validated before reading endpoint state or performing local IPC; typed replies are printed to the terminal.

## Ownership and execution

Single-process command lifetime; endpoint and request resources are scoped to one invocation.

## Invariants and errors

Keep administrative authentication and stop/wait behavior intact. Report errors to stderr and return a failing process status.

## Commands

See `minimium-platform::commands` and `--help` for current usage.

## Change guide

Change CLI argument handling here; shared syntax and help metadata belong in the commands domain.

## Verification

Run `rtk cargo check -p minimiumctl` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.
