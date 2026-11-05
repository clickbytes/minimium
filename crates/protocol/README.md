# minimium-protocol

## Purpose / Non-goals

Owns versioned network message models, byte codecs, validation and frame limits. TCP buffering and gameplay decisions belong outside this crate.

## Public API

`ClientMessage`, `ServerMessage`, `WorldContext`, protocol limits and encode/decode functions are re-exported from `src/lib.rs`.

## Dependencies

Uses core domain identifiers and values. Domain packages do not depend on protocol.

## Data flow

Typed messages encode to bounded byte frames; input bytes decode to typed messages, incomplete input or a protocol error.

## Ownership and execution

Codec calls are synchronous and own no connection or mutable session state.

## Invariants and errors

Preserve the current protocol version, framing, field limits and explicit malformed-message errors unless a separate protocol change is approved.

## Commands

None.

## Change guide

Change wire representation here, then update all message producers/consumers and version documentation together.

## Verification

Run `rtk cargo check -p minimium-protocol` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.
