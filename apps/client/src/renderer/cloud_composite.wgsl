@group(2) @binding(0) var cloud_low: texture_2d<f32>;
@group(2) @binding(1) var cloud_middle: texture_2d<f32>;
@group(2) @binding(2) var cloud_high: texture_2d<f32>;
@group(2) @binding(3) var cloud_depth: texture_2d<f32>;
@group(2) @binding(4) var scene_depth: texture_depth_2d;

fn composed_clouds(pixel: vec2<i32>) -> vec4<f32> {
    let values = array<vec4<f32>, 4>(textureLoad(cloud_low, pixel, 0),
        textureLoad(cloud_middle, pixel, 0), textureLoad(cloud_high, pixel, 0),
        textureLoad(cloud_depth, pixel, 0));
    var result = vec4<f32>(0.0);
    var transmission = 1.0;
    for (var i = 0u; i < 4u; i++) {
        let tau = values[i].a * weather.presentation.w;
        let opacity = 1.0 - exp(-tau);
        result = vec4<f32>(result.rgb + transmission * opacity * values[i].rgb / max(values[i].a, 0.000001),
            result.a + transmission * opacity);
        transmission *= exp(-tau);
    }
    return result;
}

@fragment fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let full_size = vec2<i32>(textureDimensions(scene_depth));
    let size = vec2<i32>(textureDimensions(cloud_low));
    let depth = textureLoad(scene_depth, min(vec2<i32>(position.xy), full_size - vec2<i32>(1)), 0);
    let texel = position.xy / vec2<f32>(full_size) * vec2<f32>(size) - vec2<f32>(0.5);
    let base = vec2<i32>(floor(texel));
    let fraction = fract(texel);
    var sum = vec4<f32>(0.0);
    var weights = 0.0;
    var nearest = vec4<f32>(0.0);
    var closest = 1.0e9;
    for (var y = 0; y < 2; y++) {
        for (var x = 0; x < 2; x++) {
            let pixel = clamp(base + vec2<i32>(x, y), vec2<i32>(0), size - vec2<i32>(1));
            let depth_pixel = vec2<i32>((vec2<f32>(pixel) + vec2<f32>(0.5))
                / vec2<f32>(size) * vec2<f32>(full_size));
            let sample_depth = textureLoad(scene_depth, min(depth_pixel, full_size - vec2<i32>(1)), 0);
            let difference = abs(sample_depth - depth) / max(1.0 - depth, 0.000001);
            let color = composed_clouds(pixel);
            if difference < closest { closest = difference; nearest = color; }
            let bilinear = select(1.0 - fraction.x, fraction.x, x == 1)
                * select(1.0 - fraction.y, fraction.y, y == 1);
            let weight = bilinear * exp(-difference * 100.0);
            sum += color * weight;
            weights += weight;
        }
    }
    var color = select(nearest, sum / max(weights, 0.00001), weights > 0.00001);
    if weather.presentation.x > 0.5 {
        // Compress only the final stack; keep its premultiplied color consistent.
        let opacity = min(color.a, weather.presentation.y);
        color *= opacity / max(color.a, 0.00001);
    }
    return color;
}
