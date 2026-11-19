struct TerrainInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) high: vec3<f32>,
    @location(3) low: vec3<f32>,
    @location(4) coverage_a: vec4<f32>,
    @location(5) coverage_b: vec2<f32>,
    @location(6) surface: vec4<f32>,
    @location(7) geometric_normal: vec3<f32>,
    @location(8) habitat: vec3<f32>,
    @location(9) presentation: vec2<f32>,
};
struct TerrainOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) @interpolate(flat) high: vec3<f32>,
    @location(3) low: vec3<f32>,
    @location(4) coverage_a: vec4<f32>,
    @location(5) coverage_b: vec2<f32>,
    @location(6) surface: vec4<f32>,
    @location(7) climate: vec2<f32>,
    @location(8) @interpolate(flat) geometric_normal: vec3<f32>,
    @location(9) habitat: vec3<f32>,
    @location(10) exposed_subsurface: f32,
    @location(11) sky_visibility: f32,
};
fn macro_climate(direction: vec3<f32>, elevation: f32) -> vec2<f32> {
    let latitude = asin(clamp(direction.y, -1.0, 1.0));
    let angular_latitude = latitude/1.57079632679;
    let temperature = 28.0-52.0*angular_latitude*angular_latitude
        +18.0*direction.y*terrain.climate.x-max(elevation, 0.0)*0.0065+terrain.climate.y;
    let belt_distance = (abs(latitude)-0.5)/0.18;
    let dry_belt = exp(-(belt_distance*belt_distance));
    return vec2<f32>(temperature, clamp(0.55-0.38*dry_belt+terrain.climate.z, 0.02, 0.99));
}

@vertex fn vs_terrain(input: TerrainInput) -> TerrainOutput {
    return TerrainOutput(camera.view_projection * vec4<f32>(input.position, 1.0),
        input.position, input.normal, input.high, input.low, input.coverage_a, input.coverage_b, input.surface,
        macro_climate(normalize(input.high+input.low), mix(input.surface.x,input.surface.w,terrain.climate.w)),input.geometric_normal,input.habitat,
        input.presentation.x,input.presentation.y);
}
@fragment fn fs_terrain(input: TerrainOutput) -> @location(0) vec4<f32> {
    let dx = dpdx(input.low);
    let dy = dpdy(input.low);
    let footprint = max(length(dx), length(dy));
    let patches = patch_fields(input.high, input.low, footprint);
    let base_normal = normalize(input.normal);
    let to_eye = terrain.eye_relative.xyz-input.position;
    let distance = length(to_eye);
    let view = normalize(to_eye);
    let detail = 1.0-smoothstep(0.02, 0.4, footprint);
    let direction = normalize(input.high+input.low);
    let climate_height = mix(input.surface.x,input.surface.w,terrain.climate.w);
    let weather = surface_weather(direction,climate_height,input.climate);
    let temperature = weather.temperature;
    let moisture = weather.cover.w;
    let thermal = smoothstep(-3.0, 15.0, temperature);
    var cover = weather.cover.xyz;
    cover.x = min(cover.x, thermal);
    // Snow needs precipitation history or mapped permanent ice. Cold/humidity
    // alone must not paint a full snow layer over otherwise dry ground.
    let cut = clamp(input.exposed_subsurface,0.0,1.0);
    let snow = clamp(max(cover.y,input.coverage_b.y)*(1.0-smoothstep(1.0,6.0,temperature)),0.0,1.0);
    let habitat = habitat_moisture(patches,moisture,cover.z,input.habitat.x);
    let heat_survival = 1.0-smoothstep(38.0,52.0,temperature);
    let base_weights = array<f32, 6>(input.coverage_a.x, input.coverage_a.y,
        input.coverage_a.z, input.coverage_a.w, input.coverage_b.x, input.coverage_b.y);
    var weights = base_weights;
    if cut < 1.0 {
        weights = patch_coverage(base_weights, patches, cover.x*heat_survival,
            thermal*heat_survival, habitat, input.surface.y, footprint);
        weights[4] += weights[5]*(1.0-snow);
        weights[5] = snow;
        for (var layer = 0u; layer < 5u; layer++) { weights[layer] *= 1.0-snow; }
        for (var layer = 0u; layer < 6u; layer++) {
            weights[layer] = mix(weights[layer],base_weights[layer],cut);
        }
    }
    if terrain.debug.x == 7u {
        return vec4<f32>(input.habitat.y, input.habitat.x, 0.1, 1.0);
    }
    if terrain.debug.x == 8u {
        return vec4<f32>(mix(vec3<f32>(0.8,0.25,0.05),vec3<f32>(0.05,0.65,0.8),input.habitat.z),1.0);
    }
    if terrain.debug.x == 6u {
        let colors = array<vec3<f32>,6>(vec3<f32>(0.8,0.6,0.2),vec3<f32>(0.35,0.16,0.06),
            vec3<f32>(0.05,0.8,0.1),vec3<f32>(0.5,0.2,0.05),vec3<f32>(0.35,0.4,0.5),vec3<f32>(1.0));
        var color = vec3<f32>(0.0);
        var total = 0.0;
        for (var i = 0u; i < 6u; i++) { color += weights[i]*colors[i]; total += weights[i]; }
        return vec4<f32>(color/max(total,0.0001),1.0);
    }
    let greenness = smoothstep(0.12,0.5,habitat)*heat_survival*(1.0-cut);
    let sample = blend_terrain(input.high,input.low,base_normal,weights,dx,dy,detail,greenness);
    var albedo = sample.albedo;
    let packed = sample.packed;
    let normal = sample.normal;
    let macro_variation = 1.0+(patches.x-0.5)*mix(0.16,0.04,snow)*(1.0-cut);
    var total_weight = 0.0;
    for (var i = 0u; i < 6u; i++) { total_weight += weights[i]; }
    let loose_fraction = (weights[0]+weights[1])/max(total_weight,0.0001);
    let mineral_tint = mix(vec3<f32>(0.95,0.98,1.03),vec3<f32>(1.04,1.0,0.95),patches.y);
    albedo *= mix(vec3<f32>(1.0),mineral_tint,loose_fraction*(1.0-cut));
    if cut > 0.0 {
        // The material maps supply grain, microheight and normals. Freshly
        // exposed earth has dark clods; broken rock exposes paler mineral faces.
        let grain = smoothstep(0.25,0.75,packed.b);
        let soil = mix(vec3<f32>(0.49,0.40,0.34),vec3<f32>(0.86,0.74,0.65),grain);
        let rock = mix(vec3<f32>(0.86,0.89,0.92),vec3<f32>(1.24,1.27,1.30),grain);
        let rock_fraction = weights[4]/max(weights[1]+weights[4],0.0001);
        albedo *= mix(vec3<f32>(1.0),mix(soil,rock,rock_fraction),cut);
    }
    // Signed ground height defines a narrow damp shore; water is separate geometry.
    let shore = (1.0-smoothstep(0.05,1.2,input.surface.x))
        *(1.0-snow)*(1.0-smoothstep(0.05,0.3,input.surface.y))*input.surface.z;
    let wetness = max(cover.z,shore*0.65)*(1.0-cut);
    albedo *= macro_variation * mix(1.0, 0.62, wetness);
    if terrain.debug.x == 5u { return vec4<f32>(albedo,1.0); }
    if terrain.debug.x == 1u { albedo = vec3<f32>(0.5); }
    let base_roughness = clamp(mix(packed.r, 0.16, wetness), 0.08, 1.0);
    // Broaden highlights when unresolved normal variation would otherwise sparkle.
    let normal_dx = dpdx(normal);
    let normal_dy = dpdy(normal);
    let variance = min(0.15,0.5*max(dot(normal_dx,normal_dx),dot(normal_dy,normal_dy)));
    let roughness = pow(min(1.0,pow(base_roughness,4.0)+variance),0.25);
    var visibility = 1.0;
    var lunar_direct = vec3<f32>(0.0);
    var direct = vec3<f32>(0.0);
    if length(environment.sun_geometric_world.xyz) > 1e-8 {
        let sun = sunlight_at_surface(input.position);
        let light = sun.direction;
        if shadow.flags.y == 1u { visibility = shadow_visibility(input.position, input.geometric_normal, base_normal, light); }
        direct += surface_light(normal, view, light, albedo, roughness, packed.a) * sun.irradiance * visibility;
    }
    if length(environment.moon_geometric_world.xyz) > 1e-8 {
        let moon = moonlight_at_surface(input.position);
        let light = moon.direction;
        let moon_visibility = lunar_shadow_visibility(input.position, input.geometric_normal, base_normal, light);
        if shadow.flags.y == 2u { visibility = moon_visibility; }
        lunar_direct = surface_light(normal, view, light, albedo, roughness, packed.a) * moon.irradiance * moon_visibility;
    }
    if environment.bolid_position_intensity.w > 0.0 {
        let bolt = environment.bolid_position_intensity.xyz
            - (input.position + environment.render_origin.xyz);
        let light = normalize(bolt);
        var irradiance = vec3<f32>(0.0);
        if dot(normal,light)>0.0 {irradiance=bolid_incident(input.position);}
        var bolt_visibility = 1.0;
        if shadow.flags.y == 3u {
            bolt_visibility = shadow_visibility(input.position, input.geometric_normal, input.geometric_normal, light);
            visibility = bolt_visibility;
        }
        direct += surface_light(normal, view, light, albedo, roughness, packed.a)
            * irradiance * bolt_visibility;
    }
    for (var i=0u; i<min(transient_lights.count.x,4u); i++) {
        let light=transient_direction(i,input.position);
        if dot(input.geometric_normal,light)>0.0 {
            direct+=surface_light(normal,view,light,albedo,roughness,packed.a)
                *transient_incident(i,input.position);
        }
    }
    if transient_lights.count.y!=0u {return transient_debug_transmission(input.position);}
    if terrain.debug.x == 2u { return vec4<f32>(normal*0.5+0.5, 1.0); }
    if terrain.debug.x == 3u { return vec4<f32>(vec3<f32>(input.clip.z), 1.0); }
    if terrain.debug.x == 4u {
        // Red identifies facets facing away from the point source. These are
        // geometric self-occlusion, distinct from acne on a front-facing plane.
        if shadow.flags.y == 3u {
            let delta = environment.bolid_position_intensity.xyz
                - (input.position + environment.render_origin.xyz);
            if dot(input.geometric_normal, delta) <= 0.0 {
                return vec4<f32>(1.0, 0.0, 0.0, 1.0);
            }
        }
        return vec4<f32>(vec3<f32>(visibility), 1.0);
    }
    // Cached landform concavity supplies a restrained cavity approximation.
    // It attenuates only indirect light; directional shadows remain geometric.
    let shelter=smoothstep(0.45,0.85,input.habitat.x)*(1.0-smoothstep(0.15,0.5,input.habitat.y));
    let ao = packed.g*mix(1.0,0.65,shelter)*mix(1.0,mix(0.12,1.0,input.sky_visibility),cut);
    let ambient = terrain_ambient(normal,view,direction,albedo,roughness,ao,packed.a,background_ambient(input.position),false);
    let diffuse = terrain_ambient(normal,view,direction,albedo,roughness,ao,packed.a,lunar_sky_irradiance(input.position),true);
    return terrain_light_output(input.position,lunar_direct,diffuse,ambient+direct);
}
