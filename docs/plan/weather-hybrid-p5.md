# P5 — planet-capable atmosphere profile (Earth active)

- AtmosphereProfile owns planetary geometry, gravity, rotation, orbit, insolation, pressure, temperature, condensable and all Rayleigh/Mie/ozone optical parameters.
- GasComposition has independently validated gas fractions. Scattering coefficients stay calibrated, not inferred from composition alone.
- The CPU transmittance LUT generator and the WGSL globe/cloud/orbital atmosphere sample the same coefficients, uploaded as four aligned vec4 uniforms.
- The Earth profile preserves the preexisting scattering constants. No new save or protocol change is introduced.
- WeatherSurface::uniform and profile validation enable later alternative-planet configurations in the solver.
- Other planetary environments are **not yet playable**: non-water microphysics, astronomy, runtime profile switching, shader/resource rebinding and network/save profile transport remain outside this Earth milestone.
- Source code committed; final compile, WGSL/runtime, visual and performance verification NOT RUN.
