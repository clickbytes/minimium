# minimium-localization

## Purpose / Non-goals

Owns locale identifiers, typed user-facing messages and Fluent catalog formatting for application UI. It has no window, GPU, network, or process responsibilities.

## Public API

`Locale`, `UiMessage`, and `Localizer` format the same typed message in English or Polish. The requested catalog is checked first, then English; a missing Polish entry therefore falls back to its English sentence. If neither catalog contains a key, the stable key is returned and a localization diagnostic is emitted when a logger was supplied. Technical error codes and command/property syntax remain stable data rather than translated text.

A catalog entry that fails Fluent formatting also emits a diagnostic and is skipped. The next catalog is tried; if every candidate fails, the stable message key is returned.
Catalog parsing requires exactly one default variant per Fluent select expression. If construction fails, the client startup error path prints the underlying error and the catalog failure.

## Dependencies

Uses Fluent and Unicode locale identifiers, plus the domain-neutral diagnostics contract.

## Data flow

Application code constructs a `UiMessage` with typed values and asks `Localizer` to format it for the current locale. Catalogs own sentence grammar, including plural selection.

## Ownership and execution

Applications own the selected locale and a `Localizer`; this package owns only the catalogs and formatting rules.

## Invariants and errors

Message keys are stable and independent of translated text. Technical errors and command syntax are not localized here.

## Commands

None.

## Change guide

Add a typed `UiMessage` or `UiNotice`, format its arguments in the message
contract, and add matching entries to `locales/en.ftl` and `locales/pl.ftl`.
Construct that message at a real application consumer, then format it through
the `Localizer` owned by the client application. Both desktop and terminal
frontends consume the same message; English fallback applies when a Polish key
is absent.

## Verification

Compilation and static validation are deferred to the workspace final verification phase.
