# ADR 0007: Static command registry ownership

Status: accepted.

Shared command metadata and parsing live in `minimium-commands`, which depends only on platform-neutral core values. Each feature owner declares its own typed command and parser; the registry resolves names and aliases and exposes usage metadata without centralizing every command enum. Hosts own execution and map commands to UI, protocol messages or server actions. Platform adapters may validate administrative input before I/O, but they do not define command semantics.

The current client command catalog and server administrative catalog both use the shared generic registry. The client still owns its executor and front-end actions; the server still owns administrative dispatch. Environment time value parsing belongs to `minimium-environment`; the generic registry has no domain dependencies. This keeps command syntax reusable without a service locator or general plugin framework.
