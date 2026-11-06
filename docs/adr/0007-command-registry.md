# ADR 0007: Static command registry ownership

Status: accepted.

Shared command metadata and parsing live in `minimium-commands`, which depends only on platform-neutral values. Each feature owner declares its own typed command and parser; atomic registry installation checks names and aliases together, and a parse outcome distinguishes an unknown name from invalid arguments. The crate also defines neutral property descriptors, property request syntax and structured error fields. It does not centralize domain keys, hold live setting values or validate domain-specific values. Hosts route parsed requests to each owner's typed handler and map results to UI, protocol messages or server actions. Platform adapters may validate administrative input before I/O, but they do not define command semantics.

The current client command catalog and server administrative catalog both use the shared generic registry. The client still owns its executor and front-end actions; the server still owns administrative dispatch, with host built-ins reserving names before module installation. Environment time value parsing belongs to `minimium-environment`; the generic registry has no domain dependencies. This keeps syntax reusable without a service locator or general plugin framework.
