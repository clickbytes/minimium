# ADR 0010: Keep socket byte progress below connection policy

Status: accepted for the current extraction.

`minimium-transport` owns bounded synchronous progress when writing queued bytes to a socket. The connection runtime retains framing, client identity, backpressure thresholds, deadlines, and disconnect decisions. Protocol encoding remains in `minimium-protocol`.

This boundary lets transport report partial progress and `WouldBlock` without knowing game messages. Broader connection orchestration remains in the server until it can move without carrying session and simulation policy into the transport crate.
