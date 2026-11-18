fn air_scattering_phases(cosine: f32) -> vec2<f32> {
    let g = atmosphere.mie.y;
    let angle = max(1.0 + g*g - 2.0*g*cosine, 0.001);
    return vec2<f32>(3.0*(1.0+cosine*cosine)/(16.0*3.14159265),
        (1.0-g*g)/(4.0*3.14159265*pow(angle,1.5)));
}

fn integrate_air_interval(ray: vec3<f32>, start: f32, distance: f32, steps: u32) -> AirPath {
    var result = clear_air_path();
    let radius = atmosphere.radii.x;
    let height = max(atmosphere.up_height.w,-12000.0);
    let r = radius+height;
    let up = atmosphere.up_height.xyz;
    let b = r*dot(up,ray);
    let c_top = (height-atmosphere.radii.y)*(2.0*radius+height+atmosphere.radii.y);
    let discriminant = b*b-c_top;
    if discriminant <= 0.0 { return result; }
    let root = sqrt(discriminant);
    let begin = max(-b-root,start);
    var end = min(-b+root,distance);
    let c_ground = height*(2.0*radius+height);
    // Depressions below mean sea level still contain air. The occlusion sphere
    // may sit just below the eye; actual local terrain ends rays via scene depth.
    let clipping_height=max(height,0.01);
    let c_occlusion=clipping_height*(2.0*r-clipping_height);
    let ground_d = b*b-c_occlusion;
    if b < 0.0 && ground_d > 0.0 {
        end = min(end,c_occlusion/(-b+sqrt(ground_d)));
    }
    if end <= begin { return result; }
    let sun = normalize(environment.global_sun_direction.xyz);
    // Concentrate samples around the lowest point of the visible ray segment.
    // Uniform steps miss thin near-ground fog when viewing from high altitude.
    let closest=clamp(-b,begin,end);
    var left_steps=steps/2u;
    if closest<=begin {left_steps=0u;}
    if closest>=end {left_steps=steps;}
    for (var i=0u;i<steps;i++) {
        var segment_begin=0.0;
        var segment_end=0.0;
        if i<left_steps {
            let a=1.0-f32(i)/f32(left_steps);
            let z=1.0-f32(i+1u)/f32(left_steps);
            segment_begin=closest-(closest-begin)*a*a*a;
            segment_end=closest-(closest-begin)*z*z*z;
        } else {
            let count=max(steps-left_steps,1u);
            let a=f32(i-left_steps)/f32(count);
            let z=f32(i-left_steps+1u)/f32(count);
            segment_begin=closest+(end-closest)*a*a*a;
            segment_end=closest+(end-closest)*z*z*z;
        }
        let d = (segment_begin+segment_end)*0.5;
        let ds = segment_end-segment_begin;
        let radial_difference = c_ground+2.0*b*d+d*d;
        let point_r = sqrt(max(radius*radius+radial_difference,1.0));
        let raw_height=radial_difference/(point_r+radius);
        let h = max(raw_height,0.0);
        let point_up = normalize(up*r+ray*d);
        let rayleigh = atmosphere.rayleigh.xyz*exp(-h/atmosphere.radii.z);
        let aerosol = atmosphere.mie.x*exp(-h/atmosphere.radii.w);
        let fog=regional_fog(raw_height,point_up);
        let mie = aerosol+fog;
        result.fog+=fog*ds;
        let ozone = max(1.0-abs(h-atmosphere.ozone.w)/atmosphere.mie.z,0.0);
        let extinction = rayleigh+vec3<f32>(mie)+atmosphere.ozone.xyz*ozone;
        let segment = exp(-extinction*ds);
        let apparent_sun = solar_direction(point_up,h);
        let solar_phase = air_scattering_phases(dot(ray,apparent_sun));
        let sunlight = air_light(h,point_up,apparent_sun)*environment.global_sun_rgb.xyz;
        let mu = dot(point_up,sun);
        let multi_uv = (vec2<f32>(mu*0.5+0.5,sqrt(clamp(h/atmosphere.radii.y,0.0,1.0)))*31.0+0.5)/32.0;
        let multi = textureSampleLevel(multiple_scattering,atmosphere_sampler,multi_uv,0.0).rgb*environment.global_sun_rgb.xyz;
        let moon_ray = lunar_ray(environment.moon_orbit_world.xyz-point_up*point_r/atmosphere.config.y,point_up,h);
        let moon = environment.global_moon_rgb.xyz*disk_air_light(h,point_up,moon_ray.xyz,moon_ray.w,true);
        let position = atmosphere.eye_relative.xyz + ray * (d/atmosphere.config.y);
        var solar_visibility = 1.0;
        var lunar_visibility = 1.0;
        if shadow.flags.y == 1u { solar_visibility = air_shadow_visibility(position); }
        // Planetary occlusion is already evaluated at this sample height.
        // Use terrain visibility for local fog. High clear air above the shadow
        // footprint still receives light along its own planetary ray.
        if any(moon > vec3<f32>(0.0)) && (fog > 0.0 || any(lunar_map_coverage(position) > vec2<f32>(0.0))) {
            lunar_visibility = lunar_shadow_visibility(position,vec3<f32>(0.0),vec3<f32>(0.0),moon_ray.xyz);
        }
        result.shadowed_fog += fog*ds*(1.0-select(solar_visibility,lunar_visibility,shadow.flags.y==2u));
        let lunar_phase = air_scattering_phases(dot(ray,moon_ray.xyz));
        var fog_skylight = vec3<f32>(0.0);
        if fog > 0.0 {
            // Isotropic fog lit by an upper hemisphere of equivalent mean radiance.
            fog_skylight = lunar_sky_irradiance(position)*(fog*0.9/(2.0*3.14159265));
        }
        let source = fog_skylight + (rayleigh*solar_phase.x+vec3<f32>(mie*0.9*solar_phase.y))*sunlight*solar_visibility
            +(rayleigh*lunar_phase.x+vec3<f32>(mie*0.9*lunar_phase.y))*moon*lunar_visibility
            +(rayleigh+vec3<f32>(mie*0.9))*multi;
        result.scattering += result.transmission*source/max(extinction,vec3<f32>(1e-12))*(1.0-segment);
        result.transmission *= segment;
    }
    return result;
}
