# minimium-transport

## Purpose / Non-goals

Owns transport-level socket byte flushing and bounded write progress. It does not decode game messages, manage sessions, or apply game rules.

## Public API

`flush` writes at most a byte budget from a queued byte slice, retaining the caller-owned offset across partial writes and `WouldBlock`.

## Dependencies

Uses the Rust standard library only. Protocol and runtime policy remain outside this crate.

## Data flow

The connection owner supplies a `TcpStream`, byte buffer, offset, and budget; transport returns bytes written and an optional I/O error.

## Ownership and execution

The caller owns the stream, buffer, offset and scheduling. The call is synchronous and performs no background work.

## Invariants and errors

The offset never advances past the buffer, and the write budget is respected. `WouldBlock` preserves queued bytes; a zero-byte write with pending data returns `WriteZero`.

## Commands

None.

## Change guide

Change generic socket write progression here. Frame validation, backpressure policy and connection lifecycle belong to the session runtime.

## Verification

Run `rtk cargo check -p minimium-transport -p minimium-server` and the architecture/file-length checks. Runtime acceptance is deferred.
