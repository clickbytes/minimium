# ADR 0003: Bind player snapshots to canonical map data

- Status: Accepted
- Scope: Static map compatibility and durable local player records

## Context

Persisted player positions are meaningful only against a compatible static map. The prototype also needs stable local player identity across reconnects without presenting that identifier as account authentication.

## Decision

The server computes a fingerprint from parsed canonical static map records. The client and server compare this fingerprint during the game handshake to detect accidental map mismatch; the fingerprint is not a security mechanism.

Map and OBJ parsing and file access belong to `minimium-assets`. The validated core map computes the fingerprint from spawn, boxes and expanded triangles in their existing order, normalizing signed zero as before. This boundary does not change the fingerprint format.

World snapshot format v3 records the map fingerprint and durable player profile ID, name, position, and health. The client stores a local profile ID and presents it during the protocol v8 handshake. It is a public identifier, not proof of account ownership. Version 2 snapshots migrate with full health. Protocol v8 and snapshot v3 remain fixed for the current implementation scope; any future format change needs an explicit compatibility and migration decision.

## Consequences

- Whitespace and line-ending differences in equivalent parsed map records do not by themselves change the fingerprint.
- A fingerprint mismatch prevents a client from treating the server's map as the same local map, but does not establish trust or authorship.
- Player records persist across reconnects for a local profile; they do not provide public account security.
- Inventory, durable world state, account identity, and authenticated ownership remain outside the current scope.

## Evidence boundary

This records the current compatibility contract and storage direction. It does not claim crash-proof persistence, authenticated identities, or successful recovery in all failure modes; those behaviors remain subject to deferred verification.
