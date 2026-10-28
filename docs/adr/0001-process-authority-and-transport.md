# ADR 0001: Separate client and server processes

- Status: Accepted
- Scope: Runtime ownership and game transport boundary

## Context

Local play must exercise the same authoritative server behavior as network play. Sharing world mutation between a client and server in one process would obscure that boundary and complicate server ownership. The game protocol also needs to describe messages without requiring a specific socket API.

## Decision

The client and server are separate processes, including during local play. The server is the authority for world simulation, validation, and persisted player state. A locally launched server is a child process and receives game traffic through the normal client/server protocol. The local admin endpoint is a separate loopback-only operational interface and is not part of the public game protocol.

Protocol message definitions and validation remain independent of the transport adapter. TCP is the current prototype transport. Selecting or implementing another transport is future work and does not implicitly change protocol v8.

## Consequences

- The client cannot directly mutate authoritative server state; it sends inputs and consumes server results.
- Local and remote connection paths can share the game handshake and message handling.
- The server can run headless under a service manager, independent of a client window.
- Current TCP behavior is suitable for local prototyping only. Public-game authentication, encryption, congestion handling, and a production transport require separate decisions.
- A process boundary does not by itself provide authentication or protect a public service.

## Evidence boundary

This records an architectural decision and current prototype arrangement. It is not evidence of network security, multi-process reliability, or production readiness.
