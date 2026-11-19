# minimium-commands

The public `admin` codec validates opt-in administrative v1 requests and correlated replies, preserving exact IDs and tagged command values. [Wire contract](../../docs/console-api/admin-v1.md)

## Purpose / Non-goals

Owns generic command parsing, typed parse outcomes and errors, property metadata, and the format of immutable configuration layers. It contains no domain-specific command or property vocabulary. Domain state, value validation and execution remain with feature owners and host applications.

## Public API

`operation::Effect::LocalJob` declares a host-owned local worker effect. It
does not authorize any domain mutation performed by that worker.
`syntax::bind::bind_values` prepares typed machine arguments against the same
catalog schema as canonical text, without constructing or parsing a command
line. The owning host still checks `scriptable`, capability, permission and
context before dispatch.

`CommandRegistry<T, E>` and `CommandSpec<T, E>` provide longest-name/alias matching, atomic name registration, parser dispatch and declarative argument metadata. `CommandArgumentSpec` records each argument's name, type, optional/rest behavior, enum choices or constraints, unit and description. Hosts render command help from these descriptors with `write_command_help` into a bounded writer or `format_command_help` into a `String`. `CommandOutcome` distinguishes an unknown name from a recognized command with invalid arguments. `PropertyRegistry` validates and lists static property descriptors without owning values. `PropertyRequest` parses `get`, `set`, `reset`, `list` and `help`; the requesting host checks access and calls the domain handler. `CommandError` carries a stable code, subject, reason and usage.

The public `operation` module composes `CatalogEntry` declarations through `CatalogBuilder`. A package registers atomically. The immutable catalog resolves exact paths and aliases, lists direct children and describes signatures, units, capability, access, persistence and source. The limits are 1024 operations, 2048 aliases, 128 bytes per name and 16 segments. Separate source bindings may share an identical schema. A collision identifies both owners and operation IDs. The [compiled example](examples/operation_catalog.rs) uses the production builder and help formatter.

The public syntax binder returns a prepared operation with its ID, requested access, typed arguments and source spans. Hosts validate these arguments through their existing domain constructors. Formatting an operation explanation requires only catalog metadata. The canonical parser also exposes incomplete input for editors. [Compiled syntax example](examples/operation_syntax.rs)

The `results` module carries operation identity, request, session and world binding, stage, effect receipts and typed outcome. A property report keeps startup provenance, session override, requested/effective values, owner read source, revision and freshness. Its configuration projection borrows the startup data. Machine payloads allow 32 KiB, eight levels and 1024 value nodes. The complete administrative envelope has a 64 KiB budget.

Navigation receipts may pass through Preparing after a matching owner transfer ID is reported and Ready after the client queues readiness for that ID. These stages precede the owner's membership commitment; Ready does not claim server acceptance of the readiness frame.

## Dependencies

Depends on minimium-core, ron and serde. The [public operation contract](../../docs/console-api/contract.md) defines the shared catalog and its owner bindings.

## Data flow

Feature owners register parsers, argument descriptors and property descriptors. Hosts derive command help and argument completion from the same metadata, then route parsed requests to the domain-owned validator and handler. Configuration parsing returns raw entries and source metadata; it does not mutate live domain state.

## Ownership and execution

This crate owns no live mutable domain state, I/O or worker. Its registries are immutable metadata after startup; file access and execution remain with adapters and host applications.

## Invariants and errors

Preserve longest-name matching and stable alias resolution. Keep feature-specific ranges, units and error text in their owning domains.

## Commands

Exports a generic registry, not a centralized engine command vocabulary. Feature domains register their own specifications.

## Change guide

Change registry matching and metadata behavior here. Change command values and validation in their feature domain; change execution in the host.

## Verification

Run `rtk cargo check -p minimium-commands -p minimium-platform -p minimium-server -p minimiumctl -p minimium-client` plus architecture and file-length checks. Runtime acceptance is deferred.

An administrative actor ID is an optional lossless u64 accompanying the expected world binding. A server operation decides whether that binding applies. InstantMs accepts integer milliseconds, with quoted calendar text available to domain parsers.

A settings receipt can identify its configuration base as well as the property revision. The owner checks both identifiers for conditional changes and reports the conflicting dimension.

Operation receipts can carry sample age in real milliseconds alongside freshness and revision. An actor-scoped result preserves its session identity and original world binding.

The pure completion analyzer accepts UTF-8 byte positions, a selection and a buffer revision. It uses the immutable catalog spelling index, shared literal and word tokenizers, and execution binder. Up to 64 ordered edits carry complete replacement spans and catalog metadata. Signatures identify the active argument. The catalog schema fingerprint is calculated at registration.

MAX_INPUT_BYTES defines the 512-byte public client line limit. InputTooLong carries the rejected source range. The immutable catalog resolves operation IDs retained by bounded help reports.
