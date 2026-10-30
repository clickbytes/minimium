# Astronomy Engine vendor snapshot

This directory contains the C implementation, public C header, and MIT license
from Astronomy Engine v2.1.19, pinned to commit
`61dc07020aaa6885d2c7f688a4d82beaf6edb9ef` (annotated tag object
`03084ee684bdcc490273fe85f9df4f1c8fb66199`). The files are kept verbatim;
`provenance.json` records their upstream URLs and SHA-256 digests.

The Rust crate compiles these local sources together with the project-owned
bridge in `crates/astronomy/ffi`. No network access, bindgen, or libclang is
needed during a build. A C compiler is required. The bridge defines
`ASTRONOMY_ENGINE_NO_CURRENT_TIME` so calculations do not consult the host
clock.

The checked-in Rust declarations are generated from the stable local bridge
header with:

```sh
python scripts/generate_astronomy_bindings.py
```

That project-owned generator is version 1; update it and the bridge ABI version
in `provenance.json` together when changing the ABI. It does not invoke an
external binding generator.

The upstream library is MIT licensed. Keep `LICENSE` with redistributed
upstream source. The project bridge and Rust wrapper are maintained separately
under the repository license.
