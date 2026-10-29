# ADR 0005 — Seeded, addressable world generation

## Status

Accepted for Epic 003 implementation; the generator algorithm and address format
are revisioned and must be preserved once worlds are published.

## Decision

- `minimium-core` owns the domain values for seed, generation revision, manifest,
  instance location, sector addresses and generated-object origin IDs. It does not
  execute generation or perform I/O.
- `minimium-procgen` is a pure library depending on `core`. Generation receives a
  validated `GenerationSpec`, a fixed `GenerationChannel` and a logical `SectorKey`.
- Random words come from Philox4x32-10. A versioned stable integer hash encodes the
  sector address into its counter; the 128-bit world seed supplies the 64-bit key
  and the remaining counter domain. The random result at an index does not depend
  on request order or mutable generator state.
- Logical positions use millimeters and orientation uses a canonical Q1.30
  quaternion. Rendering and collision addresses remain separate from persistent
  sector identity.
- Changing face order, address encoding, hash, channel identifiers, Philox rounds,
  quantization, or output interpretation requires a new generation revision.

## Philox reference

Philox4x32-10 uses multipliers `0xD2511F53` and `0xCD9E8D57`, Weyl key increments
`0x9E3779B9` and `0xBB67AE85`, and ten rounds. The constants and recurrence follow
the [Random123 Philox reference implementation](https://github.com/lemire/random123/blob/master/include/Random123/philox.h).

## Consequences

This gives server and client the same pure generation primitive and makes sector
results seekable. Sector-address hashing maps a larger logical address space into
the 128-bit Philox counter; it is a deterministic address encoding, not a
cryptographic identity or security boundary. Origin-ID collisions are possible in
principle and must be handled as a data-integrity failure by the world owner.
The Philox known-answer vector passed on Windows in the final Epic 003 phase.
Cross-platform generation equivalence remains unverified and requires the same
vectors on each supported target.
