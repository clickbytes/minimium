# Administrative protocol v1

After the existing AUTH line, send one UTF-8 line starting with `@console/1 `. A plain command selects the legacy response. The transport checks authentication before either format. It permits eight active clients, 512 request bytes including the marker, and 64 KiB for a response including framing. The server's two-second request deadline and the client's three-second response deadline apply in both modes.

A text request for catalog help can be written as follows.

```text
@console/1 {"schema_version":1,"correlation":{"nonce":"896231","request_id":"1"},"expected_context":null,"payload":{"kind":"text","text":"help world.time.scale"}}
```

A structured request sends the canonical ID, access, tagged argument array and optional assigned value directly to the binder.

```text
@console/1 {"schema_version":1,"correlation":{"nonce":"896231","request_id":"2"},"expected_context":null,"payload":{"kind":"operation","operation_id":"ui.console.help","access":"invoke","arguments":[{"Enum":"world.time.scale"}],"value":null}}
```

Access is `read`, `write`, `reset` or `invoke`. Only a write supplies a value. Only an invocation supplies positional arguments. A server administrative request has no implicit session actor. An expected actor world binding is rejected with `stale_context`. Malformed input, unsupported versions and authentication failures end the request before owner dispatch.

The reply uses the same marker and includes `schema_version`, `nonce` and the versioned `result`. Its identity carries the logical request ID and correlation nonce. The receipt carries stage, revision, freshness, known effects and the independent shutdown flag. A successful help payload contains the declared operation spec, aliases and source bindings. Pages retain their rows, continuation and truncation flag. A display-only handler returns `LegacyText`. Structured errors retain their code, reason, subject, usage, origin and source span where available.

Wide integers, request IDs, nonces and exact decimal coefficients use JSON strings. Typed payload and effects together allow 32 KiB. The codec validates size, nesting, element count, schema, operation result consistency and correlation. Excess data receives a typed limit error. The server keeps the highest request ID per nonce for its lifetime and rejects duplicates and stale IDs. Both numbers must be nonzero. The table holds 65,536 nonces and reports capacity exhaustion explicitly.

The client allows one pending administrative worker. Cancellation before sending is definitive. A failure after a complete request line was sent leaves the outcome unknown unless a correlated reply arrives. The client never resends an operation automatically. The game protocol is version 23 and the save format is version 12.

`minimiumctl --format=json help world.time.scale` writes a machine reply to stdout. Plain `minimiumctl help world.time.scale` uses the legacy display mode. A failed operation exits with status 1 in both modes. With an owned local server, F1, developer stdin and terminal help for a shared world property receive the server's catalog metadata. Offline help uses the local catalog.

A solar HHMM invocation includes the actor as a decimal string and expected_context with decimal-string instance, content_revision and membership_epoch. The owner rejects a missing observer with context_required and changed membership with stale_context. A clock write returns owner_committed with revision, tick, calendar epoch, requested and effective values and measured progress.
