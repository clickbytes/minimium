# P2: hybrid visual cloud presentation

## Implemented
- Near: the physical ellipsoid enclosures use detailed 3D-density view integration and density-gradient normals. `weather clouds method volumetric` is default; `method analytic` provides a visual fallback.
- Orbital: curved 2.5D layers sample opacity per pixel from the same published physical column cache used for surface weather and cloud shadows. Cache resolution is 64x64 for global/regional endpoints.
- The method switch affects GPU presentation only, never cloud IDs, weather time, rain or storm host.
- Command status includes the selected method; reset returns to volumetric.

## Constraints and pending work
- The near pass still draws overlapping lobe screen enclosures. It is not a single fullscreen ray marcher, and performance is not yet optimized.
- The orbital cache contains area moments rather than exact 3D projection: microstructures and lighting remain approximate.
- Code-only change; compilation, WGSL validation, runtime visuals and benchmarks NOT RUN.
- P3 teleport invariants, P4 stateful atmosphere and P5 planet profiles follow.
