struct SkyVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@group(1) @binding(0) var milky_way_map: texture_2d<f32>;
@group(1) @binding(1) var milky_way_sampler: sampler;

struct BolidGpuEvent {
    start_age: vec4<f32>,
    end_duration: vec4<f32>,
    style: vec4<f32>,
    head_power: vec4<f32>,
    light_position_power: vec4<f32>,
    emission: vec4<f32>,
    bounds: vec4<f32>,
    fragments: array<vec4<f32>, 19>,
    tails: array<vec4<f32>, 20>,
    optics: array<vec4<f32>, 20>,
    trail: array<vec4<f32>, 24>,
    trail_meta: array<vec4<f32>, 24>,
};
struct BolidGpuFrame {
    events: array<BolidGpuEvent, 8>,
    count: vec4<u32>,
};
@group(1) @binding(2) var<uniform> bolids: BolidGpuFrame;
@group(2) @binding(0) var bolid_scene_depth: texture_depth_2d;
var<private> bolid_pixel: vec2<i32>;
var<private> bolid_distance_sum: vec2<f32>;
var<private> bolid_pixel_angle: f32;
struct BolidOutput {
    @location(0) color: vec4<f32>,
    @location(1) distance: vec4<f32>,
};
fn record_bolid_distance(point: vec3<f32>, radiance: vec3<f32>) -> vec3<f32> {
    let weight=min(dot(radiance,vec3<f32>(0.2126,0.7152,0.0722)),100.0);
    bolid_distance_sum+=vec2<f32>(length(point-camera.eye_world.xyz)/1000000.0,1.0)*weight;
    return radiance;
}
fn bolid_uv(position: vec2<f32>) -> vec2<f32> {
    let uv=position/vec2<f32>(bolids.count.yz);
    bolid_pixel=vec2<i32>(uv*camera.viewport.xy);
    return uv;
}

fn bolid_in_bounds(ray: vec3<f32>, bounds: vec4<f32>) -> bool {
    let delta=bounds.xyz-camera.eye_world.xyz;
    let nearest=delta-ray*max(dot(delta,ray),0.0);
    // Physical bounds do not include the minimum angular/pixel footprint.
    // Keep the whole filtered core and train support at orbital distances.
    let angular_support=max(0.004,6.0/max(f32(bolids.count.z),1.0));
    let radius=bounds.w+(length(delta)+bounds.w)*angular_support;
    return dot(nearest,nearest)<=radius*radius;
}

fn bolid_transmission(point: vec3<f32>) -> vec3<f32> {
    if physical_atmosphere() {
        return sample_aerial(point-environment.render_origin.xyz).transmission;
    }
    // Match the finite RGB path used by the point light. The analytic sky's
    // grey, infinite-column approximation ignored the world-to-optical scale
    // and could attenuate orbital views even when both endpoints are in vacuum.
    return bolid_air_transmission(camera.eye_world.xyz,point);
}

@fragment fn fs_bolid(@builtin(position) position: vec4<f32>) -> BolidOutput {
    let ray=atmosphere_ray(bolid_uv(position.xy));
    // Evaluate derivatives before event-dependent control flow. The actual
    // angular footprint follows both the render resolution and camera FOV.
    bolid_pixel_angle=max(length(dpdx(ray)),length(dpdy(ray)));
    let emission=bolid_radiance(ray);
    // Keep emission linear until foreground-cloud extinction is known, even
    // when the main scene uses the analytic/display fallback.
    return BolidOutput(vec4<f32>(bounded_hdr_rgb(emission,30000.0),0.0),vec4<f32>(bolid_distance_sum,0.0,0.0));
}

fn bolid_visible(point: vec3<f32>) -> bool {
    let local=point-environment.render_origin.xyz;
    let clip=camera.view_projection*vec4<f32>(local,1.0);
    if clip.w<=0.0 {return false;}
    let stored=textureLoad(bolid_scene_depth,bolid_pixel,0);
    if stored<1.0 && clip.z/clip.w>stored+0.0000002 {return false;}
    let eye = camera.eye_world.xyz;
    let delta = point-eye;
    let distance = length(delta);
    let radius = atmosphere.config.x;
    let ray = delta/max(distance,1.0);
    let b = dot(eye,ray);
    let c = (length(eye)-radius)*(length(eye)+radius);
    let d = b*b-c;
    if d < 0.0 || radius <= 0.0 { return true; }
    let hit = -b-sqrt(d);
    return hit <= 0.0 || hit >= distance;
}
fn bolid_spot(ray: vec3<f32>, point: vec3<f32>, axis: vec3<f32>, energy: f32, optics: vec4<f32>) -> vec3<f32> {
    if energy <= 0.0 { return vec3<f32>(0.0); }
    let delta = point-camera.eye_world.xyz;
    let distance = max(length(delta),100.0);
    let angle = length(ray-delta/distance);
    let pixel = max(bolid_pixel_angle,1e-7);
    let physical_width=max(optics.x,2.0/atmosphere.config.y)/distance;
    let width = max(physical_width,pixel*0.65);
    let dilution=min(pow(physical_width/width,2.0),1.0);
    if angle>width*9.0 { return vec3<f32>(0.0); }
    if !bolid_visible(point) { return vec3<f32>(0.0); }
    let core = exp(-pow(angle/width,2.0)*1.8);
    let tangent=axis-dot(axis,delta/distance)*delta/distance;
    let behind=ray-delta/distance+tangent*width*1.8;
    let expansion=1.0+0.8*exp(-optics.z/0.09);
    let asymmetry=0.85+0.15*sin(dot(behind,vec3<f32>(13.0,29.0,47.0))/width);
    let sheath = exp(-dot(behind,behind)/(width*width*3.5*expansion))*asymmetry;
    let halo = exp(-pow(angle/(width*6.0),2.0)*1.5);
    let flux_scale=625.0/max(pow(optics.x*atmosphere.config.y,2.0),4.0);
    return vec3<f32>(core*80.0,sheath*8.0/expansion,halo*0.15)*energy*dilution*flux_scale;
}

// Odd approximation of erf (maximum error about 5e-4). Integrating a finite
// Gaussian line gives half its weight at an endpoint: adjoining segments then
// sum to one, instead of adding two full round caps and producing bright beads.
fn bolid_erf(x: f32) -> f32 {
    let a=min(abs(x),8.0);
    let p=1.0+a*(0.278393+a*(0.230389+a*(0.000972+a*0.078108)));
    let p2=p*p;
    return sign(x)*(1.0-1.0/(p2*p2));
}

fn bolid_radiance(ray: vec3<f32>) -> vec3<f32> {
    var result = vec3<f32>(0.0);
    for (var index=0u; index<min(bolids.count.x,8u); index++) {
        let e=bolids.events[index];
        if e.start_age.w<0.0 || !bolid_in_bounds(ray,e.bounds) { continue; }
        let color=e.emission.xyz;
        let axis=normalize(e.end_duration.xyz-e.start_age.xyz);
        let spot=bolid_spot(ray,e.head_power.xyz,axis,e.emission.w,e.optics[0]);
        if any(spot>vec3<f32>(0.0)) {
            result += record_bolid_distance(e.head_power.xyz,
                (vec3<f32>(spot.x)+color*(spot.y+spot.z))*bolid_transmission(e.head_power.xyz));
        }
        for(var f=0u;f<20u;f++) {
            var head=e.head_power;
            if f>0u { head=e.fragments[f-1u]; }
            let optics=e.optics[f];
            let tail=e.tails[f];
            if max(head.w,tail.w)<=0.000001 { continue; }
            let direction=normalize(head.xyz-tail.xyz+axis*0.001);
            let tint=mix(vec3<f32>(1.0,0.47,0.21),color,optics.y);
            if f>0u {
                let fragment=bolid_spot(ray,head.xyz,direction,head.w,optics);
                if any(fragment>vec3<f32>(0.0)) {
                    result += record_bolid_distance(head.xyz,
                        (vec3<f32>(fragment.x)+tint*(fragment.y+fragment.z))*bolid_transmission(head.xyz));
                }
            }
            let av=tail.xyz-camera.eye_world.xyz;
            let bv=head.xyz-camera.eye_world.xyz;
            let da=normalize(av);
            let db=normalize(bv);
            let edge=db-da;
            if dot(edge,edge)<1e-12 { continue; }
            let t=clamp(dot(ray-da,edge)/dot(edge,edge),0.0,1.0);
            let angle=length(ray-normalize(mix(da,db,t)));
            let elapsed=fract(optics.w)*(1.0-t);
            let initial_width=max(optics.x,8.0/atmosphere.config.y);
            let wake_width=initial_width+elapsed*8.0/atmosphere.config.y;
            let physical_width=wake_width/max(length(mix(av,bv,t)),100.0);
            let width=max(physical_width,bolid_pixel_angle*0.65);
            let dilution=min(physical_width/max(width,1e-8),1.0);
            if angle>width*4.0 || !bolid_visible(mix(tail.xyz,head.xyz,t)) { continue; }
            let grain_nodes=1.0-floor(optics.w)*0.35*(0.5+0.5*sin(t*37.0+f32(f)*2.7));
            // Same 70%/0.25 s fresh-emission handoff as bolid_model.rs.
            // Old gas returns smoothly to the common cooling train.
            let share=0.7*(1.0-smoothstep(0.18,0.25,elapsed));
            let power=mix(tail.w,head.w,t)*exp(-elapsed/0.45)
                *share*initial_width/wake_width;
            result += record_bolid_distance(mix(tail.xyz,head.xyz,t),
                tint*power*1.5*grain_nodes*dilution*exp(-pow(angle/width,2.0)*1.6)
                *bolid_transmission(mix(tail.xyz,head.xyz,t)));
        }
        for(var i=0u;i<23u;i++) {
            let a=e.trail[i];
            let b=e.trail[i+1u];
            let av=a.xyz-camera.eye_world.xyz;
            let bv=b.xyz-camera.eye_world.xyz;
            let da=normalize(av);
            let db=normalize(bv);
            let edge=db-da;
            if dot(edge,edge)<1e-12 { continue; }
            let along=dot(ray-da,edge)/dot(edge,edge);
            let t=clamp(along,0.0,1.0);
            let angle=length(ray-normalize(mix(da,db,t)));
            let deposited=mix(e.trail_meta[i].x,e.trail_meta[i+1u].x,t);
            let elapsed=max(e.start_age.w-deposited,0.0);
            let corrugation=1.0+0.14*sin(deposited*35.0+e.style.y*19.0);
            let width=mix(e.trail_meta[i].z,e.trail_meta[i+1u].z,t)*corrugation/max(length(mix(av,bv,t)),100.0);
            let filtered_width=max(width,bolid_pixel_angle*0.65);
            if angle>filtered_width*5.0 { continue; }
            if !bolid_visible(mix(a.xyz,b.xyz,t)) { continue; }
            // A line spreads in one dimension (a core in two). Filtering must
            // not create energy as a distant trail becomes subpixel-sized.
            let longitudinal=length(edge)*along;
            let gaussian_scale=1.264911064/filtered_width;
            let segment_weight=max(0.0,0.5*(bolid_erf(longitudinal*gaussian_scale)
                +bolid_erf((length(edge)-longitudinal)*gaussian_scale)));
            let transverse=length(ray-normalize(da+edge*along));
            let profile=exp(-pow(transverse/filtered_width,2.0)*1.6)*segment_weight
                *min(width/max(filtered_width,1e-8),1.0);
            let chemistry=mix(e.trail_meta[i].w,e.trail_meta[i+1u].w,t);
            // Day-adapted contrast suppresses both the chemical afterglow and
            // older cooling gas. Applying it only to chemistry left an orange
            // self-luminous line seconds after a daytime head had vanished.
            // Fresh flight remains visible; dust scatters daylight separately.
            let daylight=smoothstep(-0.12,0.04,environment.sky_zenith_rgb_sun_altitude.w);
            let old_gas=smoothstep(0.35,2.0,elapsed);
            let train_contrast=1.0-max(chemistry,old_gas)*daylight*0.998;
            let cooling_color=mix(color,vec3<f32>(0.85,0.55,0.3),min(elapsed/4.0,0.55));
            result += record_bolid_distance(mix(a.xyz,b.xyz,t),
                mix(cooling_color,vec3<f32>(0.56,0.72,0.48),chemistry)
                * profile*mix(a.w,b.w,t)*1.5*train_contrast*bolid_transmission(mix(a.xyz,b.xyz,t)));
        }
    }
    return result;
}

// Dust scatters external daylight; it is not an additive glowing night trail.
@fragment fn fs_dust(@builtin(position) position: vec4<f32>) -> BolidOutput {
    let uv=bolid_uv(position.xy);
    let daylight=smoothstep(-0.23,0.04,environment.sky_zenith_rgb_sun_altitude.w);
    if daylight <= 0.0 { discard; }
    let ndc=uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0);
    let far_point=camera.inverse_sky_view_projection*vec4<f32>(ndc,1.0,1.0);
    let ray=normalize(far_point.xyz/max(abs(far_point.w),1e-6));
    var opacity=0.0;
    var illumination=vec3<f32>(0.0);
    for(var index=0u;index<min(bolids.count.x,8u);index++) {
        let e=bolids.events[index];
        if e.style.w<=0.0 || e.start_age.w<=0.0 || !bolid_in_bounds(ray,e.bounds) { continue; }
        for(var i=0u;i<23u;i++) {
            let a=e.trail[i].xyz;
            let b=e.trail[i+1u].xyz;
            let av=a-camera.eye_world.xyz;
            let bv=b-camera.eye_world.xyz;
            let da=normalize(av);
            let db=normalize(bv);
            let edge=db-da;
            if dot(edge,edge)<1e-12 { continue; }
            let t=clamp(dot(ray-da,edge)/max(dot(edge,edge),1e-12),0.0,1.0);
            let deposited=mix(e.trail_meta[i].x,e.trail_meta[i+1u].x,t);
            let elapsed=max(e.start_age.w-deposited,0.0);
            let width=max((80.0+elapsed*18.0)/(atmosphere.config.y*max(length(mix(av,bv,t)),100.0)),0.0005);
            let angle=length(ray-normalize(mix(da,db,t)));
            if angle>width*4.0 || !bolid_visible(mix(a,b,t)) { continue; }
            let density=smoothstep(0.0,2.0,elapsed)*exp(-elapsed/30.0)
                *mix(e.trail_meta[i].y,e.trail_meta[i+1u].y,t)
                *80.0/(80.0+elapsed*18.0);
            let contribution=0.22*density*exp(-pow(angle/width,2.0));
            if contribution>opacity {
                opacity=contribution;
                let point=mix(a,b,t);
                // Both output paths use sunlight at the dust altitude. The
                // optical-depth table is available without HDR/compute too;
                // a constant grey fill would illuminate dust in Earth's shadow.
                illumination=physical_sun(point-environment.render_origin.xyz)
                    *vec3<f32>(0.65,0.68,0.72)/3.14159265
                    *bolid_transmission(point);
            }
            _ = record_bolid_distance(mix(a,b,t),vec3<f32>(contribution));
        }
    }
    // Linear scattered daylight, composed once in the scene output space.
    return BolidOutput(vec4<f32>(tone_map_linear(illumination),opacity*daylight),
        vec4<f32>(bolid_distance_sum,0.0,0.0));
}

// The embedded plate's luminance median is ~0.014 and its 90th percentile
// ~0.062. Gains of 0.02/0.03 put most of it below two 8-bit display levels
// after the shared filmic tone map. Keep diffuse galactic contrast in linear
// scene space; daylight, moonlight, extinction and cloud occlusion still apply.
const MILKY_WAY_SURFACE_GAIN: f32 = 0.075;
const MILKY_WAY_ORBIT_GAIN: f32 = 0.17;

fn milky_way_radiance(ray_world: vec3<f32>) -> vec3<f32> {
    let eqj = normalize((transpose(environment.eqj_to_world) * vec4<f32>(ray_world, 0.0)).xyz);
    // NASA SVS celestial plate carree: RA 0 at the centre, RA increases left.
    let u = fract(0.5 - atan2(eqj.y, eqj.x) / 6.2831853);
    let v = 0.5 - asin(clamp(eqj.z, -1.0, 1.0)) / 3.14159265;
    let source = textureSample(milky_way_map, milky_way_sampler, vec2<f32>(u, v)).rgb;
    let luminance = dot(source, vec3<f32>(0.2126, 0.7152, 0.0722));
    // Dark-adapted vision sees little colour in diffuse galactic light.
    let diffuse = max((luminance - 0.008) * 1.35, 0.0);
    return mix(vec3<f32>(diffuse), source * diffuse / max(luminance, 0.001), 0.12);
}

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> SkyVertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var output: SkyVertexOutput;
    output.clip_position = vec4<f32>(positions[index], 1.0, 1.0);
    return output;
}

@fragment fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let ndc = vec2<f32>(
        position.x * camera.viewport.z * 2.0 - 1.0,
        1.0 - position.y * camera.viewport.w * 2.0,
    );
    let far_point = camera.inverse_sky_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let ray_world = normalize(far_point.xyz / max(abs(far_point.w), 1.0e-6));
    if physical_atmosphere() {
        let galaxy = milky_way_radiance(ray_world)*MILKY_WAY_ORBIT_GAIN*horizon_visibility(ray_world)*celestial_contrast();
        return vec4<f32>(galaxy,1.0);
    }
    if environment.flags.y == 1u {
        return vec4<f32>(tone_map_linear(milky_way_radiance(ray_world) * MILKY_WAY_ORBIT_GAIN), 1.0);
    }
    let sample = sample_clear_sky(ray_world);
    let moon_flux = max(environment.moon_disk_radiance.w, 0.0)
        * environment.local_moon_rgb_visibility.w;
    let moon_contrast = 1.0 / (1.0 + 8.0 * moon_flux);
    let galactic_visibility = sample.transmittance
        * celestial_contrast() * moon_contrast;
    var galactic_light = vec3<f32>(0.0);
    if celestial_contrast() > 0.0 {
        galactic_light = milky_way_radiance(ray_world);
    }
    let sky = tone_map_linear(sample.scattering_rgb
        + galactic_light * MILKY_WAY_SURFACE_GAIN * galactic_visibility
);
    let ground = HORIZON_OCCLUSION_RGB;
    let visibility = horizon_visibility(ray_world);
    return vec4<f32>(mix(ground, sky, visibility), 1.0);
}
