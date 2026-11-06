# minimium-protocol

## Purpose / Non-goals

Owns versioned network message models, byte codecs, validation and frame limits. TCP buffering and gameplay decisions belong outside this crate.

## Public API

The current protocol version is 17. Version 15 added `Presentation`, `ContentOffer` and `ContentReady`. The offer carries
a required material manifest SHA-256 (zero selects base), separate HTTP port and
bounded climate scenario; the ready message echoes the hash. Desktop admission
waits for content readiness. Headless capabilities bypass material preparation.
The scenario is a validated binary record with at most 32 circular regions.

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


Version 16 adds WorldInput subtype 8, `SetAltitude` (finite `f32` kilometres,
0..10000), and server type 0x91, `AltitudeResult` (world context, success flag,
optional three i64 millimetre coordinates). Current peers must both use v16.
Server gameplay validation additionally requires fly mode and a planet.
No generation revision or save-format change accompanies this protocol change.

Version 17 extends every present planet definition with four u32 peak-relief
fields (start_m, full_m, multiplier_permille, exponent). ClimateScenario in
ContentOffer adds an f64 relief_influence after global moisture, before the region
count. Both bounds and generator compatibility are validated on reception.
All peers must use v17; older wire versions are rejected explicitly.
