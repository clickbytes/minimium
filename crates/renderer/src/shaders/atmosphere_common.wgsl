fn disk_area_above(center_in_radii: f32) -> f32 {
    if center_in_radii <= -1.0 {return 0.0;}
    if center_in_radii >= 1.0 {return 1.0;}
    let t = center_in_radii;
    let height = 1.0-abs(t);
    if height < 0.01 {
        let cap = (4.0*sqrt(2.0)/(3.0*3.14159265))*height*sqrt(height)*(1.0-0.15*height);
        return select(cap,1.0-cap,t>0.0);
    }
    return (acos(-t) + t*sqrt(max(1.0-t*t,0.0)))/3.14159265;
}

fn disk_horizon_coordinate(site: vec4<f32>, ray: vec4<f32>) -> f32 {
    let h = max(site.w,0.0);
    let r = atmosphere.radii.x+h;
    let horizon = -sqrt(h*(2.0*atmosphere.radii.x+h))/r;
    return (dot(site.xyz,ray.xyz)-horizon)/max(ray.w*atmosphere.radii.x/r,1e-7);
}
// Stable spherical integration in optical metres.
fn disk_air_light(height: f32, up: vec3<f32>, light: vec3<f32>, sine_radius: f32, lunar: bool) -> vec3<f32> {
    if sine_radius<=0.0 || atmosphere.radii.x<=0.0 || atmosphere.radii.y<=0.0 {return vec3<f32>(0.0);}
    let h = max(height,0.0);
    let r = atmosphere.radii.x+h;
    let horizon = -sqrt(clamp(h*(2.0*atmosphere.radii.x+h)/(r*r),0.0,1.0));
    var cosine = clamp(dot(up,light),-1.0,1.0);
    let half_width=max(sine_radius*atmosphere.radii.x/r,1e-7);
    let disk = select(smoothstep(horizon-half_width,horizon+half_width,cosine),
        disk_area_above((cosine-horizon)/half_width),lunar);
    if disk<=0.0 {return vec3<f32>(0.0);}
    let lookup_height=min(h,atmosphere.radii.y);
    var lookup_horizon=horizon;
    if h>lookup_height {
        let shell_radius=atmosphere.radii.x+lookup_height;
        let discriminant=r*r*cosine*cosine-(h-lookup_height)*(r+shell_radius);
        if cosine>=0.0 || discriminant<=0.0 {return vec3<f32>(disk);}
        // Start the LUT ray where it enters the outer atmosphere.
        cosine=-sqrt(discriminant)/shell_radius;
        lookup_horizon=-sqrt(lookup_height*(2.0*atmosphere.radii.x+lookup_height))/shell_radius;
    }
    let u = sqrt(clamp((cosine-lookup_horizon)/(1.0-lookup_horizon),0.0,1.0));
    let v = sqrt(lookup_height/atmosphere.radii.y);
    let uv = (vec2<f32>(u,v)*vec2<f32>(127.0,63.0)+0.5)/vec2<f32>(128.0,64.0);
    return exp(-textureSampleLevel(atmosphere_table,atmosphere_sampler,uv,0.0).rgb)*disk;
}
fn air_light(height: f32, up: vec3<f32>, light: vec3<f32>) -> vec3<f32> {
    return disk_air_light(height,up,light,sin(environment.sun_apparent_world.w),false);
}
fn point_height(local: vec3<f32>) -> vec4<f32> {
    let offset = (local-atmosphere.eye_relative.xyz)*atmosphere.config.y;
    let r = atmosphere.radii.x+atmosphere.up_height.w;
    let difference = atmosphere.up_height.w*(2.0*atmosphere.radii.x+atmosphere.up_height.w)
        +2.0*r*dot(atmosphere.up_height.xyz,offset)+dot(offset,offset);
    let length = sqrt(max(atmosphere.radii.x*atmosphere.radii.x+difference,1.0));
    return vec4<f32>(normalize(atmosphere.up_height.xyz*r+offset),difference/(length+atmosphere.radii.x));
}
struct CelestialLight { direction: vec3<f32>, irradiance: vec3<f32> };
fn sunlight_at_surface(local:vec3<f32>) -> CelestialLight {
    if !has_spherical_atmosphere() {
        return CelestialLight(environment.sun_apparent_world.xyz,environment.local_sun_rgb_visibility.xyz);
    }
    let site = point_height(local);
    let direction=solar_direction(site.xyz,site.w);
    return CelestialLight(direction,environment.global_sun_rgb.xyz*air_light(site.w,site.xyz,direction));
}
fn physical_sun(local:vec3<f32>)->vec3<f32> {
    return sunlight_at_surface(local).irradiance;
}
fn surface_moon_ray(local: vec3<f32>, site: vec4<f32>) -> vec4<f32> {
    let to_moon=(environment.moon_orbit_world.xyz-environment.render_origin.xyz)-local;
    return lunar_ray(to_moon,site.xyz,site.w);
}
fn background_ambient(local: vec3<f32>) -> vec3<f32> {
    if !physical_atmosphere() { return environment.ambient_rgb_exposure.xyz; }
    let site = point_height(local);
    let solar_cosine=dot(site.xyz,normalize(environment.global_sun_direction.xyz));
    let day=smoothstep(-0.04,0.25,solar_cosine);
    // At solar depression a, sunlight clears the solid planet above R*(sec(a)-1).
    // Use the illuminated fraction of the air column, including a weak, broader
    // multiple-scattered tail. An angle-only linear fill overwhelms a dark-adapted eye.
    let below=max(-solar_cosine,0.0);
    let cos_depression=sqrt(max(1.0-below*below,1e-8));
    let shadow_height=atmosphere.radii.x*below*below/(cos_depression*(1.0+cos_depression));
    let shadow_depth=max(shadow_height-max(site.w,0.0),0.0)/atmosphere.radii.z;
    let twilight=(1.0-day)*(0.95*exp(-shadow_depth)+0.05*exp(-0.5*shadow_depth));
    let air_density=exp(-max(site.w,0.0)/8000.0);
    let solar=environment.global_sun_rgb.xyz
        *(vec3<f32>(0.10,0.13,0.17)*day+vec3<f32>(0.012,0.010,0.009)*twilight);
    return (vec3<f32>(0.03,0.04,0.06)*1e-6
        +solar)*air_density;
}
fn lunar_sky_irradiance(local: vec3<f32>) -> vec3<f32> {
    if !has_spherical_atmosphere() || !any(environment.global_moon_rgb.xyz > vec3<f32>(0.0)) {
        return vec3<f32>(0.0);
    }
    let site = point_height(local);
    if site.w >= atmosphere.radii.y { return vec3<f32>(0.0); }
    let geometric = normalize((environment.moon_orbit_world.xyz-environment.render_origin.xyz)-local);
    let mu = clamp(dot(site.xyz,geometric),-1.0,1.0);
    let uv = vec2<f32>(0.5+0.5*sign(mu)*sqrt(abs(mu)),sqrt(clamp(site.w/atmosphere.radii.y,0.0,1.0)));
    let size = vec2<f32>(textureDimensions(sky_irradiance));
    let unit = textureSampleLevel(sky_irradiance,atmosphere_sampler,(uv*(size-1.0)+0.5)/size,0.0).rgb;
    let gain = select(max(environment.star_animation.z,1.0),1.0,physical_atmosphere());
    return unit * environment.global_moon_rgb.xyz * gain * local_sky_access(site);
}
fn physical_ambient(local: vec3<f32>) -> vec3<f32> {
    return background_ambient(local) + lunar_sky_irradiance(local);
}
fn moonlight_at_surface(local:vec3<f32>) -> CelestialLight {
    let gain=select(max(environment.star_animation.z,1.0),1.0,physical_atmosphere());
    if !has_spherical_atmosphere() {
        return CelestialLight(environment.moon_apparent_world.xyz,environment.local_moon_rgb_visibility.xyz*gain);
    }
    let site=point_height(local);
    let moon=surface_moon_ray(local,site);
    let t = disk_horizon_coordinate(site,moon);
    var direction = moon.xyz;
    if t > -1.0 && t < 1.0 {
        // Centroid of the remaining circular segment keeps the last sliver above
        // the local tangent for diffuse incidence and specular reflection.
        let lift = moon.w*(2.0/(3.0*3.14159265))*pow(max(1.0-t*t,0.0),1.5)/max(disk_area_above(t),1e-7);
        let vertical = site.xyz-moon.xyz*dot(moon.xyz,site.xyz);
        direction = normalize(moon.xyz+vertical*inverseSqrt(max(dot(vertical,vertical),1e-8))*lift);
    }
    return CelestialLight(direction,environment.global_moon_rgb.xyz*disk_air_light(site.w,site.xyz,moon.xyz,moon.w,true)*gain);
}
// Positive z is the published cube face. Its cyclic minor axes stay fixed in world space.
fn cube_face_coordinates(up:vec3<f32>, signed_axis:f32) -> vec3<f32> {
    let axis = u32(abs(signed_axis))-1u;
    let depth = up[axis]*sign(signed_axis);
    return vec3<f32>(up[(axis+1u)%3u]/max(depth,0.001), up[(axis+2u)%3u]/max(depth,0.001), depth);
}

// Optical metres above sea level and confidence in the local visible surface.
fn relief_column(up:vec3<f32>, coarse:f32) -> vec3<f32> {
    let tile = atmosphere.relief_tile;
    if tile.w == 0.0 { return vec3<f32>(coarse,0.0,0.0); }
    let face = cube_face_coordinates(up,tile.w);
    let cell = (face.xy-tile.xy)/tile.z;
    let extent = vec2<f32>(textureDimensions(atmosphere_fog));
    let margin = min(cell,extent-1.0-cell);
    let edge = min(margin.x,margin.y);
    if face.z <= 0.0 || edge <= 0.0 { return vec3<f32>(coarse,0.0,0.0); }
    let ground = textureSampleLevel(atmosphere_fog,atmosphere_sampler,(cell+0.5)/extent,2,0.0);
    let weight = smoothstep(0.0,3.0,edge)*ground.z;
    let height = (ground.x*32.0+ground.y)*atmosphere.config.y/max(ground.z,0.001);
    return vec3<f32>(mix(coarse,height,weight),weight,ground.w*smoothstep(0.0,3.0,edge));
}

fn regional_fog(height: f32, up:vec3<f32>) -> f32 {
    if atmosphere.fog_time.y<=0.0 || atmosphere.fog_time.z<=0.0 {return 0.0;}
    let projection=dot(up,atmosphere.fog_center.xyz);
    if projection<=0.0 {return 0.0;}
    let face_uv=vec2<f32>(dot(up,atmosphere.fog_east.xyz),dot(up,atmosphere.fog_north.xyz))/projection;
    let local=(face_uv-vec2<f32>(atmosphere.fog_east.w,atmosphere.fog_north.w))/atmosphere.fog_center.w;
    let edge=max(abs(local.x),abs(local.y));
    if edge>=1.0 {return 0.0;}
    let uv=(clamp(local*0.5+0.5,vec2<f32>(0.0),vec2<f32>(1.0))*8.0+0.5)/vec2<f32>(textureDimensions(atmosphere_fog));
    let earlier=textureSampleLevel(atmosphere_fog,atmosphere_sampler,uv,0,0.0);
    let later=textureSampleLevel(atmosphere_fog,atmosphere_sampler,uv,1,0.0);
    let t=atmosphere.fog_time.x;
    let parameters=mix(earlier,later,t*t*(3.0-2.0*t));
    let fade=(1.0-smoothstep(0.8,1.0,edge))*atmosphere.fog_time.y;
    let haze=parameters.w*exp(-max(height,0.0)/1200.0);
    if parameters.x<=0.0 {return haze*fade;}
    let ground=relief_column(up,parameters.z);
    // A broad inversion can fill a local hollow while a ridge protrudes from it.
    let layer=max(parameters.y,1.0);
    let thickness=clamp(layer+0.5*(parameters.z-ground.x),layer*0.5,layer*2.0);
    let above=height-ground.x;
    let exterior=mix(1.0,smoothstep(-1.0,0.0,above),ground.y);
    return (exterior*parameters.x*exp(-max(above,0.0)/thickness)+haze)*fade;
}

fn local_sky_access(site: vec4<f32>) -> f32 {
    let ground = relief_column(site.xyz,site.w);
    // Fade the surface estimate over the sampled relief footprint as the receiver rises.
    let tile_radius = atmosphere.relief_tile.z*atmosphere.radii.x*16.0;
    let lifted = max(site.w-ground.x,0.0)/max(tile_radius,1.0);
    return 1.0-ground.z*(1.0-smoothstep(0.0,1.0,lifted));
}

fn relief_moon_fraction(site: vec4<f32>, moon: vec4<f32>) -> f32 {
    if !has_spherical_atmosphere() || atmosphere.relief_tile.w == 0.0 || moon.w <= 0.0 {return 1.0;}
    let mu = dot(site.xyz,moon.xyz);
    let tangent = moon.xyz-site.xyz*mu;
    if dot(tangent,tangent) < 1e-6 {return 1.0;}
    let along = normalize(tangent);
    let radius = atmosphere.radii.x + site.w;
    let step = atmosphere.relief_tile.z*radius;
    var slope = -1e4;
    for (var i=1u;i<=24u;i++) {
        let d = f32(i)*step;
        let up = normalize(site.xyz+along*d/radius);
        let ground = relief_column(up,site.w);
        if ground.y <= 0.0 {break;}
        let rise = ground.x-site.w - d*d/(2.0*radius);
        slope = max(slope,rise/d);
    }
    let planet = disk_area_above(disk_horizon_coordinate(site,moon));
    let ridge = disk_area_above((asin(clamp(mu,-1.0,1.0))-atan(slope))/moon.w);
    return min(1.0,ridge/max(planet,1e-7));
}
