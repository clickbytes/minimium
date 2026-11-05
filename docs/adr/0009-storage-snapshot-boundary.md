# ADR 0009: Persistence consumes immutable save inputs

Status: accepted for the current extraction.

`minimium-storage` owns save formats, validation, the player store and the bounded persistence worker. It does not know about the server's live world registry. The server owner lane creates a `WorldSaveInput` snapshot and passes it to storage; storage converts that input to an immutable `SaveSnapshot` before queueing disk work.

This keeps runtime coordination in the server and durable encoding/I/O in storage. The current save format and ordering remain unchanged. Storage errors are typed `io::Error` values at the boundary; queue saturation is `WouldBlock`, and the server owns reporting and retry policy.
