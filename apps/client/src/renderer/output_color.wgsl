fn tone_map_color(linear: vec3<f32>) -> vec3<f32> {
    let value = max(linear, vec3<f32>(0.0));
    let numerator = value * (value * 2.51 + vec3<f32>(0.03));
    let denominator = value * (value * 2.43 + vec3<f32>(0.59)) + vec3<f32>(0.14);
    return clamp(numerator / denominator, vec3<f32>(0.0), vec3<f32>(1.0));
}

fn srgb_to_linear(srgb: vec3<f32>) -> vec3<f32> {
    let value = clamp(srgb, vec3<f32>(0.0), vec3<f32>(1.0));
    let low = value / 12.92;
    let high = pow((value + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(high, low, value <= vec3<f32>(0.04045));
}

fn linear_to_srgb(linear: vec3<f32>) -> vec3<f32> {
    let value = max(linear, vec3<f32>(0.0));
    let low = value * 12.92;
    let high = 1.055 * pow(value, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(high, low, value <= vec3<f32>(0.0031308));
}
