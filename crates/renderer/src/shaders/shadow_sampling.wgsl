// Orthographic shadow projection has orthogonal scaled rows. Inverting each
// row gives world displacement per UV/depth unit; intersect with the triangle
// plane analytically. No subtraction of nearly equal screen-space depth values.
fn receiver_depth_gradient(normal: vec3<f32>, m: mat4x4<f32>) -> vec2<f32> {
    let u = vec3<f32>(m[0][0],m[1][0],m[2][0])*0.5;
    let v = vec3<f32>(m[0][1],m[1][1],m[2][1])*(-0.5);
    let z = vec3<f32>(m[0][2],m[1][2],m[2][2]);
    // Keep the analytic plane correction until the projection is effectively
    // singular. Fading it while retaining the full PCF kernel compares taps
    // against the wrong receiver depth and creates grazing-light acne bands.
    let normal_length_squared = dot(normal, normal);
    let z_length_squared = dot(z, z);
    let u_length_squared = dot(u, u);
    let v_length_squared = dot(v, v);
    if normal_length_squared <= 1e-20 || z_length_squared <= 1e-20
        || u_length_squared <= 1e-20 || v_length_squared <= 1e-20 {
        return vec2<f32>(0.0);
    }
    let light_facing = abs(dot(normal * inverseSqrt(normal_length_squared),
        z * inverseSqrt(z_length_squared)));
    // Below this normalized dot product the plane/light intersection is
    // numerically singular; avoid dividing by a value dominated by f32 error.
    if light_facing <= 1e-6 {
        return vec2<f32>(0.0);
    }
    let nz = dot(normal,z)/max(dot(z,z),1e-20);
    if abs(nz) <= 1e-20 {
        return vec2<f32>(0.0);
    }
    let gradient = -vec2<f32>(dot(normal,u)/max(dot(u,u),1e-20),
        dot(normal,v)/max(dot(v,v),1e-20))/nz;
    return gradient;
}

// Perspective projection maps a receiver plane to a plane in NDC. Project
// two world tangents with the local quotient-rule Jacobian, then solve its
// depth slope. PCF taps must compare the plane at their own texel centers.
fn point_receiver_depth_gradient(normal: vec3<f32>, clip: vec4<f32>, m: mat4x4<f32>) -> vec2<f32> {
    let n2 = dot(normal, normal);
    if n2 <= 1e-20 { return vec2<f32>(0.0); }
    let n = normal * inverseSqrt(n2);
    let axis = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), abs(n.x) > 0.8);
    let tangent = normalize(cross(n, axis));
    let bitangent = cross(n, tangent);
    let a = m * vec4<f32>(tangent, 0.0);
    let b = m * vec4<f32>(bitangent, 0.0);
    let ndc = clip.xyz / clip.w;
    let da = (a.xyz - ndc * a.w) / clip.w;
    let db = (b.xyz - ndc * b.w) / clip.w;
    let plane = cross(da, db);
    let scale = length(plane);
    if scale <= 1e-20 || abs(plane.z) <= scale * 0.00001 {
        return vec2<f32>(0.0);
    }
    return vec2<f32>(-2.0 * plane.x, 2.0 * plane.y) / plane.z;
}

fn sample_shadow_map(layer: i32, cascade: ShadowUniform,
    world_position: vec3<f32>, plane_normal: vec3<f32>, bias_normal: vec3<f32>, light_direction: vec3<f32>) -> f32 {
    let receiver_offset = world_position - cascade.receiver_center_relative_radius.xyz;
    let receiver_radius = cascade.receiver_center_relative_radius.w;
    let projection_position = select(world_position, receiver_offset, cascade.flags.y == 3u);
    let clip = cascade.light_view_projection * vec4<f32>(projection_position, 1.0);
    if clip.w <= 0.000001 {
        return 1.0;
    }
    let ndc = clip.xyz / clip.w;
    if ndc.x < -1.0 || ndc.x > 1.0 || ndc.y < -1.0 || ndc.y > 1.0 || ndc.z < 0.0 || ndc.z > 1.0 {
        return 1.0;
    }

    // Shared vertex normals give adjacent terrain facets a continuous displacement.
    // The rasterized triangle plane is still used for per-texel depth correction.
    let normal = select(bias_normal, plane_normal, dot(bias_normal, plane_normal) < 0.0);
    let normal_length_squared = dot(normal, normal);
    let light_length_squared = dot(light_direction, light_direction);
    let slope = select(1.0,
        1.0 - clamp(dot(normal * inverseSqrt(max(normal_length_squared, 1e-20)),
            light_direction * inverseSqrt(max(light_length_squared, 1e-20))), 0.0, 1.0),
        normal_length_squared > 1e-20 && light_length_squared > 1e-20);
    var bias_m = min(cascade.bias_fade.z, cascade.bias_fade.x + cascade.bias_fade.y * slope);
    if cascade.flags.y == 3u {
        // PCF taps may land on a neighboring facet, where extrapolating this
        // receiver plane is insufficient. Bound the separation by half a
        // receiver texel and the existing maximum world-space bias.
        let texel_m = 2.02 * receiver_radius * cascade.texel_depth_range.x;
        bias_m = min(cascade.bias_fade.z, max(bias_m, texel_m * 0.5));
    }
    // At sunrise an offset along the light ray gives almost no separation
    // from the casting plane. Displace in world metres and project both UV
    // and depth from the same point for directional and perspective maps.
    var offset = light_direction * inverseSqrt(max(light_length_squared,1e-20)) * bias_m;
    if normal_length_squared > 1e-20 {
        let n = normal * inverseSqrt(normal_length_squared);
        // Keep the outward bias continuous as the Sun crosses a facet's horizon.
        // The finite flash retains its two-sided emitter-facing offset.
        let reverse = cascade.flags.y == 3u && dot(n, light_direction) < 0.0;
        offset = n * select(bias_m, -bias_m, reverse);
    }
    let biased = cascade.light_view_projection * vec4<f32>(projection_position + offset,1.0);
    if biased.w <= 0.000001 { return 1.0; }
    let reference_depth = biased.z/biased.w;
    let uv = vec2<f32>(biased.x/biased.w*0.5+0.5, 0.5-biased.y/biased.w*0.5);
    var depth_gradient = receiver_depth_gradient(plane_normal, cascade.light_view_projection);
    if cascade.flags.y == 3u {
        depth_gradient = point_receiver_depth_gradient(plane_normal, biased, cascade.light_view_projection);
    }
    let dimensions = vec2<i32>(textureDimensions(shadow_depth));
    let receiver_texel = vec2<i32>(floor(uv * vec2<f32>(dimensions)));
    var pcf_sum = 0.0;
    var sample_count = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let sample_texel = receiver_texel + vec2<i32>(x, y);
            // Continuous tent weights: entering/leaving taps have zero weight.
            let delta = abs(vec2<f32>(sample_texel)+0.5-uv*vec2<f32>(dimensions));
            let tent = max(vec2<f32>(1.5)-delta,vec2<f32>(0.0));
            let weight = tent.x*tent.y;
            if any(sample_texel < vec2<i32>(0)) || any(sample_texel >= dimensions) {
                pcf_sum += weight;
            } else {
                // Compare against the same texel center where rasterization stored depth.
                // Correcting at an unsnapped UV leaves a sawtooth error inside every texel.
                let center_uv = (vec2<f32>(sample_texel) + 0.5) / vec2<f32>(dimensions);
                // The neighboring texel can belong to a completely different
                // triangle. At grazing angles extending the plane away from
                // the light can put it metres behind the actual receiver and
                // turn distant background terrain into a foreground blocker.
                let plane_depth = reference_depth + min(dot(depth_gradient, center_uv - uv),0.0);
                let stored_depth = textureLoad(shadow_depth, sample_texel, layer, 0);
                // Cleared depth has no blocker.
                pcf_sum += select(0.0, weight, stored_depth == 1.0 || plane_depth <= stored_depth);
            }
            sample_count += weight;
        }
    }

    return pcf_sum / max(sample_count, 1.0);
}

fn shadow_visibility(world_position: vec3<f32>, plane_normal: vec3<f32>, bias_normal: vec3<f32>, light_direction: vec3<f32>) -> f32 {
    if shadow.flags.x == 0u { return 1.0; }
    let near_weight = shadow_coverage(shadow, world_position);
    var near_visibility = 1.0;
    if near_weight > 0.0 {
        near_visibility = sample_shadow_map(0, shadow, world_position, plane_normal, bias_normal, light_direction);
    }
    if shadow.flags.y == 3u {
        let strength = clamp(bitcast<f32>(shadow.flags.z), 0.0, 1.0);
        return mix(1.0, near_visibility, near_weight * strength);
    }
    var far_visibility = 1.0;
    if near_weight < 1.0 && landscape_shadow.flags.x != 0u && landscape_shadow.flags.y == shadow.flags.y {
        let far_weight = shadow_coverage(landscape_shadow, world_position);
        if far_weight > 0.0 {
            let occlusion = sample_shadow_map(1, landscape_shadow, world_position, plane_normal, bias_normal, light_direction);
            far_visibility = mix(1.0, occlusion, far_weight);
        }
    }
    return mix(far_visibility, near_visibility, near_weight);
}

// Inspect columns vertically below the upper-limb projection. For a depth
// separation d and vertical offset y the blocker subtends y/d below that limb.
// The most elevated blocker clips a circular segment of the source.
fn lunar_cascade(layer: i32, map: ShadowUniform, p: vec3<f32>, plane: vec3<f32>, normal: vec3<f32>,
    ray: vec3<f32>, up: vec3<f32>, planet_fraction: f32) -> f32 {
    let rim = sample_shadow_map(layer,map,p,plane,normal,ray);
    let radius = bitcast<f32>(map.flags.w);
    if radius <= 0.0 || rim == 0.0 {return rim;}
    let matrix = map.light_view_projection;
    let clip = (matrix*vec4<f32>(p,1.0)).xyz;
    let uv = clip.xy*vec2<f32>(0.5,-0.5)+0.5;
    let down = (matrix*vec4<f32>(-up,0.0)).xy*vec2<f32>(0.5,-0.5);
    let size = vec2<f32>(textureDimensions(shadow_depth));
    let pixel = 1.0/max(length(down*size),1e-8);
    let span = map.texel_depth_range.w-map.texel_depth_range.z;
    let reach = max(clip.z,0.0)*span;
    let width = 2.0*radius;
    let limit = reach*width;
    let spacing = max(2.0,pow(max(limit/pixel,1.0),1.0/11.0));
    var gap = width;
    var y = pixel;
    var previous = 0.0;
    var best = vec3<f32>(0.0);
    for (var i=0u;i<12u && previous<limit;i++) {
        let offset = min(y,limit);
        let coord = vec2<i32>(floor((uv+down*offset)*size));
        if any(coord<vec2<i32>(0)) || any(coord>=vec2<i32>(size)) {break;}
        let depth = textureLoad(shadow_depth,coord,layer,0);
        let distance = (clip.z-depth)*span;
        if distance*width > pixel && previous < gap*distance {
            if offset/distance < gap || best.z == 0.0 {
                gap = min(gap,offset/distance);
                best = vec3<f32>(previous,offset,distance);
            }
        }
        previous = offset;
        y *= spacing;
    }
    if best.z > 0.0 {
        // Refine only the strongest ridge bracket.
        for (var j=0u;j<4u;j++) {
            let mid = (best.x+best.y)*0.5;
            let xy = vec2<i32>(floor((uv+down*mid)*size));
            let depth = textureLoad(shadow_depth,clamp(xy,vec2<i32>(0),vec2<i32>(size)-1),layer,0);
            if (clip.z-depth)*span >= best.z*0.99 {best.y = mid;} else {best.x = mid;}
        }
        gap = min(gap,best.y/best.z);
    }
    let terrain_fraction = rim * disk_area_above(gap/radius-1.0);
    // Planet and ridge both cover the bottom of the disk. The direct irradiance
    // already includes the planet fraction, so apply only additional occlusion.
    return min(terrain_fraction/max(planet_fraction,1e-7),1.0);
}

// Primary and secondary lunar maps use identical projections and published casters.
// A valid but uncovered landscape cannot certify an unobstructed ray.
fn lunar_shadow_visibility(p: vec3<f32>, plane: vec3<f32>, normal: vec3<f32>, ray: vec3<f32>) -> f32 {
    var near = secondary_moon;
    var far = secondary_landscape;
    var layer = 2;
    if shadow.flags.y == 2u {
        near = shadow;
        far = landscape_shadow;
        layer = 0;
    }
    if !any(environment.global_moon_rgb.xyz > vec3<f32>(0.0)) {return 0.0;}
    if !has_spherical_atmosphere() && near.flags.x == 0u {return 1.0;}
    let site = point_height(p);
    let moon = surface_moon_ray(p,site);
    var planet_fraction = 1.0;
    if has_spherical_atmosphere() {
        planet_fraction = disk_area_above(disk_horizon_coordinate(site,moon));
        if planet_fraction <= 0.0 {return 0.0;}
    }
    if near.flags.x == 0u {
        // The low quality profile has no raster shadows. A pending standard
        // publication has no certified caster set and must not expose the valley.
        if shadow.flags.x != 0u || shadow.flags.w != 0u {return 0.0;}
        return relief_moon_fraction(site,moon);
    }
    let weights = lunar_map_coverage(p);
    var contact = 0.0;
    if weights.x > 0.0 {
        contact = lunar_cascade(layer,near,p,plane,normal,ray,site.xyz,planet_fraction);
    }
    if weights.x >= 1.0 { return contact; }
    var landscape = 0.0;
    if weights.y > 0.0 {
        landscape = weights.y * lunar_cascade(layer+1,far,p,plane,normal,ray,site.xyz,planet_fraction);
    }
    return mix(landscape,contact,weights.x);
}
