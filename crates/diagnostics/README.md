# minimium-diagnostics

## Purpose / Non-goals

Defines bounded, language-neutral diagnostic events and domain loggers. It owns no files, clock, process configuration or telemetry backend.

## Public API

`DomainLogger` emits typed events through a shared `DiagnosticSink`. Domain names come from the closed `DomainId` registry.

## Dependencies

Uses only the Rust standard library. The contract must not depend on platform or telemetry infrastructure.

## Data flow

Applications create a sink and pass a fixed-domain logger to the code they compose. Event fields are capped at 8 KiB and 32 fields before they reach the sink; file backends must also cap the fully encoded record.

## Ownership and execution

The application owns sink lifetime. Domain code only constructs bounded events and calls the nonblocking sink contract.

## Invariants and errors

Event codes are stable machine identifiers, independent of user-facing translations. Oversized content is truncated and marked. The telemetry JSONL backend accounts for JSON escaping and keeps the serialized record within 8 KiB.

## Change guide

Add a `DomainId` only for a concrete event owner, then add its stable event codes
to the closed registry and create a fixed-domain `DomainLogger` at the
application composition boundary. Pass that logger into the operation; domain
code must not select a file or backend. `apps/server` and both client
frontends create loggers for composed operations, and `minimium-telemetry`
connects them to the process sink.

## Commands

None.

## Verification

Compilation and static validation are deferred to the workspace final verification phase.
