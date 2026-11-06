// Stable spherical integration in optical metres.
fn air_light(height: f32, up: vec3<f32>, light: vec3<f32>) -> vec3<f32> {
    let h = clamp(height,0.0,atmosphere.radii.y);
    let r = atmosphere.radii.x+h;
    let horizon = -sqrt(max(h*(2.0*atmosphere.radii.x+h)/(r*r),0.0));
    let cosine = dot(up,light);
    let disk = smoothstep(horizon-0.00465,horizon+0.00465,cosine);
    if disk<=0.0 {return vec3<f32>(0.0);}
    let u = sqrt(clamp((cosine-horizon)/(1.0-horizon),0.0,1.0));
    let v = sqrt(h/atmosphere.radii.y);
    let uv = (vec2<f32>(u,v)*vec2<f32>(127.0,63.0)+0.5)/vec2<f32>(128.0,64.0);
    return exp(-textureSampleLevel(atmosphere_table,atmosphere_sampler,uv,0.0).rgb)*disk;
}
fn global_moon_direction(point: vec3<f32>) -> vec3<f32> {
    return normalize(environment.moon_orbit_world.xyz-point);
}
fn global_moon_irradiance() -> vec3<f32> {
    return vec3<f32>(0.25,0.32,0.5)*(2e-6*environment.moon_disk_radiance.w);
}
fn point_height(local: vec3<f32>) -> vec4<f32> {
    let offset = (local-atmosphere.eye_relative.xyz)*atmosphere.config.y;
    let r = atmosphere.radii.x+atmosphere.up_height.w;
    let difference = atmosphere.up_height.w*(2.0*atmosphere.radii.x+atmosphere.up_height.w)
        +2.0*r*dot(atmosphere.up_height.xyz,offset)+dot(offset,offset);
    let length = sqrt(max(atmosphere.radii.x*atmosphere.radii.x+difference,1.0));
    return vec4<f32>(normalize(atmosphere.up_height.xyz*r+offset),difference/(length+atmosphere.radii.x));
}
fn physical_sun(local: vec3<f32>) -> vec3<f32> {
    let site = point_height(local);
    return environment.global_sun_rgb.xyz*air_light(site.w,site.xyz,normalize(environment.global_sun_direction.xyz));
}
fn physical_ambient(local: vec3<f32>) -> vec3<f32> {
    let site = point_height(local);
    let solar_cosine=dot(site.xyz,normalize(environment.global_sun_direction.xyz));
    let day=smoothstep(-0.04,0.25,solar_cosine);
    let twilight=smoothstep(-0.309,-0.04,solar_cosine)*(1.0-day);
    let air_density=exp(-max(site.w,0.0)/8000.0);
    // Weak airglow plus scattered moonlight, not a permanent daylight-sized fill.
    let lunar=global_moon_irradiance()*max(dot(site.xyz,surface_moon_direction()),0.0)*0.12;
    return (vec3<f32>(0.03,0.04,0.06)*1e-6+lunar
        +vec3<f32>(0.10,0.13,0.17)*day+vec3<f32>(0.012,0.010,0.009)*twilight)*air_density;
}
fn physical_moon(local:vec3<f32>) -> vec3<f32> {
    let site=point_height(local);
    return global_moon_irradiance()*air_light(site.w,site.xyz,surface_moon_direction());
}
fn integrate_air(ray: vec3<f32>, distance: f32, steps: u32) -> AirPath {
    var result = AirPath(vec3<f32>(0.0),vec3<f32>(1.0),0.0);
    let radius = atmosphere.radii.x;
    let height = max(atmosphere.up_height.w,-12000.0);
    let r = radius+height;
    let up = atmosphere.up_height.xyz;
    let b = r*dot(up,ray);
    let c_top = (height-atmosphere.radii.y)*(2.0*radius+height+atmosphere.radii.y);
    let discriminant = b*b-c_top;
    if discriminant <= 0.0 { return result; }
    let root = sqrt(discriminant);
    let begin = max(-b-root,0.0);
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
    let cosine = dot(ray,sun);
    let phase_r = 3.0/(16.0*3.14159265)*(1.0+cosine*cosine);
    let g = atmosphere.mie.y;
    let phase_m = (1.0-g*g)/(4.0*3.14159265*pow(max(1.0+g*g-2.0*g*cosine,0.001),1.5));
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
        let sunlight = air_light(h,point_up,sun)*environment.global_sun_rgb.xyz;
        let mu = dot(point_up,sun);
        let multi_uv = (vec2<f32>(mu*0.5+0.5,sqrt(clamp(h/atmosphere.radii.y,0.0,1.0)))*31.0+0.5)/32.0;
        let multi = textureSampleLevel(multiple_scattering,atmosphere_sampler,multi_uv,0.0).rgb*environment.global_sun_rgb.xyz;
        let moon_direction = global_moon_direction(point_up*point_r/atmosphere.config.y);
        let moon = global_moon_irradiance()*air_light(h,point_up,moon_direction);
        let source = (rayleigh*phase_r+vec3<f32>(mie*0.9*phase_m))*sunlight
            +(rayleigh+vec3<f32>(mie*0.9))*(multi+moon*0.08);
        result.scattering += result.transmission*source/max(extinction,vec3<f32>(1e-12))*(1.0-segment);
        result.transmission *= segment;
    }
    return result;
}

fn regional_fog(height: f32, up:vec3<f32>) -> f32 {
    if atmosphere.fog_time.y<=0.0 || atmosphere.fog_time.z<=0.0 {return 0.0;}
    let projection=max(dot(up,atmosphere.fog_center.xyz),0.01);
    let local=vec2<f32>(dot(up,atmosphere.fog_east.xyz),dot(up,atmosphere.fog_north.xyz))
        /max(projection*atmosphere.fog_center.w,0.00001);
    let edge=max(abs(local.x),abs(local.y));
    if edge>=1.0 {return 0.0;}
    let uv=(clamp(local*0.5+0.5,vec2<f32>(0.0),vec2<f32>(1.0))*8.0+0.5)/9.0;
    let earlier=textureSampleLevel(atmosphere_fog,atmosphere_sampler,uv,0,0.0);
    let later=textureSampleLevel(atmosphere_fog,atmosphere_sampler,uv,1,0.0);
    let t=atmosphere.fog_time.x;
    let parameters=mix(earlier,later,t*t*(3.0-2.0*t));
    let density=parameters.x*exp(-max(height-parameters.z,0.0)/max(parameters.y,1.0))
        +parameters.w*exp(-max(height,0.0)/1200.0);
    return density*(1.0-smoothstep(0.8,1.0,edge))*atmosphere.fog_time.y;
}
