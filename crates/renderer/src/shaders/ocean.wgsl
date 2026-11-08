struct OceanOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

@vertex fn vs_ocean(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> OceanOutput {
    return OceanOutput(camera.view_projection*vec4<f32>(position,1.0),position,normal);
}

@fragment fn fs_ocean(input: OceanOutput) -> @location(0) vec4<f32> {
    let n = normalize(input.normal);
    let v = normalize(terrain.eye_relative.xyz-input.position);
    let albedo = select(vec3<f32>(0.007,0.027,0.042),vec3<f32>(0.5),terrain.debug.x == 1u);
    // Calm water with a broad, filtered highlight; no wave textures or temporal noise.
    let variance = min(0.15,0.5*max(dot(dpdx(n),dpdx(n)),dot(dpdy(n),dpdy(n))));
    // Both Earth cameras may reverse winding. World-space facing remains stable.
    if dot(n,v) <= 0.0 { discard; }
    let roughness = pow(min(1.0,pow(0.22,4.0)+variance),0.25);
    var direct = vec3<f32>(0.0);
    var visibility = 1.0;
    if length(environment.sun_geometric_world.xyz) > 1e-8 {
        let l = normalize(select(environment.sun_geometric_world.xyz,environment.global_sun_direction.xyz,physical_atmosphere()));
        if shadow.flags.y == 1u { visibility = shadow_visibility(input.position,n,l); }
        var irradiance = environment.local_sun_rgb_visibility.xyz;
        if physical_atmosphere() { irradiance = physical_sun(input.position); }
        direct += surface_light(n,v,l,albedo,roughness,0.02)*irradiance*visibility;
    }
    if length(environment.moon_geometric_world.xyz) > 1e-8 {
        let l = normalize(select(environment.moon_geometric_world.xyz,surface_moon_direction(),physical_atmosphere()));
        var moon_visibility = 1.0;
        if shadow.flags.y == 2u { moon_visibility = shadow_visibility(input.position,n,l); visibility = moon_visibility; }
        var irradiance = environment.local_moon_rgb_visibility.xyz;
        if physical_atmosphere() { irradiance = physical_moon(input.position); }
        direct += surface_light(n,v,l,albedo,roughness,0.02)*irradiance*moon_visibility;
    }
    if environment.bolid_position_intensity.w > 0.0 {
        let bolt = environment.bolid_position_intensity.xyz
            - (input.position + environment.render_origin.xyz);
        let irradiance = bolid_incident(input.position);
        var bolt_visibility = 1.0;
        if shadow.flags.y == 3u { bolt_visibility = shadow_visibility(input.position, n, bolt); }
        direct += surface_light(n, v, normalize(bolt), albedo, roughness, 0.02)
            * irradiance * bolt_visibility;
    }
    if terrain.debug.x == 2u { return vec4<f32>(n*0.5+0.5,1.0); }
    if terrain.debug.x == 3u { return vec4<f32>(vec3<f32>(input.clip.z),1.0); }
    if terrain.debug.x == 4u { return vec4<f32>(vec3<f32>(visibility),1.0); }
    if terrain.debug.x >= 5u { return vec4<f32>(albedo,1.0); }
    var ambient = environment.ambient_rgb_exposure.xyz;
    var daylight = 1.0;
    if physical_atmosphere() {
        ambient = physical_ambient(input.position);
        daylight = smoothstep(-0.12,0.12,dot(n,normalize(environment.global_sun_direction.xyz)));
    }
    let sky_height = sqrt(clamp(dot(reflect(-v,n),n),0.0,1.0));
    var sky = mix(environment.sky_horizon_rgb_star_visibility.xyz,
        environment.sky_zenith_rgb_sun_altitude.xyz,sky_height)*daylight;
    if physical_atmosphere() {sky=ambient*mix(1.4,2.2,sky_height);}
    let fresnel = 0.02+0.98*pow(1.0-max(dot(n,v),0.0),5.0);
    let reflected = mix(ambient*0.08,sky,0.8)*fresnel;
    return vec4<f32>(tone_map_linear(albedo*ambient*(1.0-fresnel)+reflected+direct),1.0);
}
