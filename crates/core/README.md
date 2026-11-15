# minimium-core

## Purpose / Non-goals

Owns shared identifiers, player profiles, authenticated principal values and server-granted permission bits. It has no project dependencies and must not acquire world runtime, physics, clock policy, filesystem, network, process or GPU responsibilities.

The Principal permission set is supplied by the server's authentication
provider, never by client wire data. Gameplay, flight, movement-policy and
teleport permissions are separate bits; local administrative commands use the
platform's authenticated admin endpoint rather than a gameplay Principal.

## Public API

`minimium_core::{identity, ids}`. Example: use `minimium_core::ids::PlayerId` for runtime identity.

## Dependencies

No other workspace package. This keeps foundational values usable by all domains.

## Data flow

Applications and domain packages pass typed values through core APIs; protocol and persistence perform their own boundary encoding.

## Ownership and execution

Pure values and algorithms are caller-owned. World and environment domain APIs live in their respective packages.

## Invariants and errors

Keep identifiers stable and independent of runtime policy and domain implementations.

## Commands

None. Command ownership belongs to the feature domain.

## Change guide

Change shared identifiers here. Do not add world, environment, protocol or platform adapters to the foundational package.

## Verification

Run `rtk cargo check -p minimium-core` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.
