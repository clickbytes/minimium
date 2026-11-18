struct OutputParams {
    exposure: f32,
    hdr: f32,
    surface_is_srgb: f32,
    underwater: f32,
    storage_ev: f32,
    minimum_ev: f32,
    maximum_ev: f32,
    // Elapsed real seconds, -1 for a new view, -2 for fixed exposure.
    elapsed: f32,
    // Fraction of elapsed sky time attributable to fast-forward, above real time.
    @align(16) fast_forward: f32,
    // Absolute exposure divided by storage gain, zero for the adapted output.
    fixed_gain: f32,
};
struct EyeExposure {
    adapted_ev: f32,
    multiplier: f32,
    incident_ev: f32,
    target_ev: f32,
    @align(16) ceiling_ev: f32,
};
