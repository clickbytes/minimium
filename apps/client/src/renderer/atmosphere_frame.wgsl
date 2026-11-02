struct AtmosphereUniform {
    radii: vec4<f32>,
    rayleigh: vec4<f32>,
    mie: vec4<f32>,
    ozone: vec4<f32>,
    config: vec4<f32>,
};
@group(1) @binding(0) var<uniform> atmosphere: AtmosphereUniform;
@group(1) @binding(1) var atmosphere_table: texture_2d<f32>;
@group(1) @binding(2) var atmosphere_sampler: sampler;
