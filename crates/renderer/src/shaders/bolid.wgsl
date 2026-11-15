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
fn bolid_eye() -> vec3<f32> {
    return atmosphere.eye_relative.xyz;
}
fn record_bolid_distance(point: vec3<f32>, radiance: vec3<f32>) -> vec3<f32> {
    let weight=min(dot(radiance,vec3<f32>(0.2126,0.7152,0.0722)),100.0);
    bolid_distance_sum+=vec2<f32>(length(point-bolid_eye())/1000000.0,1.0)*weight;
    return radiance;
}
fn bolid_uv(position: vec2<f32>) -> vec2<f32> {
    let uv=position/vec2<f32>(bolids.count.yz);
    bolid_pixel=vec2<i32>(uv*camera.viewport.xy);
    return uv;
}

fn bolid_transmission(point: vec3<f32>) -> vec3<f32> {
    if physical_atmosphere() {
        return sample_aerial(point).transmission;
    }
    // Match the finite RGB path used by the point light. The analytic sky's
    // grey, infinite-column approximation ignored the world-to-optical scale
    // and could attenuate orbital views even when both endpoints are in vacuum.
    return bolid_air_transmission(camera.eye_world.xyz,point+environment.render_origin.xyz);
}

fn bolid_visible(point: vec3<f32>) -> bool {
    let clip=camera.view_projection*vec4<f32>(point,1.0);
    if clip.w<=0.0 {return false;}
    let stored=textureLoad(bolid_scene_depth,bolid_pixel,0);
    if stored<1.0 && clip.z/clip.w>stored+0.0000002 {return false;}
    let eye = bolid_eye();
    let delta = point-eye;
    let distance = length(delta);
    let radius = atmosphere.config.x;
    let ray = delta/max(distance,1.0);
    let world_eye=camera.eye_world.xyz;
    let b = dot(world_eye,ray);
    let c = (length(world_eye)-radius)*(length(world_eye)+radius);
    let d = b*b-c;
    if d < 0.0 || radius <= 0.0 { return true; }
    let hit = -b-sqrt(d);
    return hit <= 0.0 || hit >= distance;
}
fn bolid_spot(ray: vec3<f32>, point: vec3<f32>, axis: vec3<f32>, energy: f32, optics: vec4<f32>) -> vec3<f32> {
    if energy <= 0.0 { return vec3<f32>(0.0); }
    let delta = point-bolid_eye();
    let distance = max(length(delta),100.0);
    let angle = length(ray-delta/distance);
    let pixel = max(bolid_pixel_angle,1e-7);
    let physical_width=max(optics.x,2.0/atmosphere.config.y)/distance;
    let width = max(physical_width,pixel*0.65);
    let dilution=min(pow(physical_width/width,2.0),1.0);
    if angle>width*9.0 { return vec3<f32>(0.0); }
    if !bolid_visible(point) { return vec3<f32>(0.0); }
    let core = exp(-pow(angle/width,2.0)*5.0);
    let tangent=axis-dot(axis,delta/distance)*delta/distance;
    let behind=ray-delta/distance+tangent*width*1.8;
    let expansion=1.0+0.8*exp(-optics.z/0.09);
    let asymmetry=0.85+0.15*sin(dot(behind,vec3<f32>(13.0,29.0,47.0))/width);
    let sheath = exp(-dot(behind,behind)/(width*width*3.5*expansion))*asymmetry;
    let halo = exp(-pow(angle/(width*6.0),2.0)*1.5);
    let flux_scale=625.0/max(pow(optics.x*atmosphere.config.y,2.0),4.0);
    return vec3<f32>(core*36.0,sheath*6.0/expansion,halo*0.08)*energy*dilution*flux_scale;
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

@fragment fn fs_bolid(input: BolidVertex) -> BolidOutput {
    let ray=atmosphere_ray(bolid_uv(input.clip_position.xy));
    bolid_pixel_angle=max(length(dpdx(ray)),length(dpdy(ray)));
    var emission=vec3<f32>(0.0);
    if input.part<20u {emission=bolid_core(ray,input.event,input.part);}
    else if input.part<40u {emission=bolid_wake(ray,input.event,input.part-20u);}
    else {emission=bolid_train(ray,input.event,input.part-40u);}
    // 8 * 63 additive primitives reserve 30000 of the shared half-float layer.
    return BolidOutput(vec4<f32>(bounded_hdr_rgb(emission,30000.0/504.0),0.0),
        vec4<f32>(bolid_distance_sum,0.0,0.0));
}

fn bolid_core(ray: vec3<f32>, index: u32, f: u32) -> vec3<f32> {
    let color=bolids.events[index].emission.xyz;
    let axis=normalize(bolids.events[index].end_duration.xyz-bolids.events[index].start_age.xyz);
    var head=bolids.events[index].head_power;
    if f>0u { head=bolids.events[index].fragments[f-1u]; }
    let optics=bolids.events[index].optics[f];
    let tint=mix(vec3<f32>(1.0,0.47,0.21),color,optics.y);
    let spot=bolid_spot(ray,head.xyz,axis,head.w,optics);
    if !any(spot>vec3<f32>(0.0)) {return vec3<f32>(0.0);}
    return record_bolid_distance(head.xyz,
        (mix(tint,vec3<f32>(1.0),0.65)*spot.x+tint*(spot.y+spot.z))
        *bolid_transmission(head.xyz));
}
fn bolid_wake(ray: vec3<f32>, index: u32, f: u32) -> vec3<f32> {
    var head=bolids.events[index].head_power;
    if f>0u {head=bolids.events[index].fragments[f-1u];}
    let tail=bolids.events[index].tails[f];
    let optics=bolids.events[index].optics[f];
    let tint=mix(vec3<f32>(1.0,0.47,0.21),bolids.events[index].emission.xyz,optics.y);
    let av=tail.xyz-bolid_eye();
    let bv=head.xyz-bolid_eye();
    let da=normalize(av);
    let db=normalize(bv);
    let edge=db-da;
    if dot(edge,edge)<1e-12 { return vec3<f32>(0.0); }
    let t=clamp(dot(ray-da,edge)/dot(edge,edge),0.0,1.0);
    let angle=length(ray-normalize(mix(da,db,t)));
    let elapsed=fract(optics.w)*(1.0-t);
    let initial_width=max(optics.x,8.0/atmosphere.config.y);
    let wake_width=initial_width+elapsed*8.0/atmosphere.config.y;
    let physical_width=wake_width/max(length(mix(av,bv,t)),100.0);
    let width=max(physical_width,bolid_pixel_angle*0.65);
    let dilution=min(physical_width/max(width,1e-8),1.0);
    if angle>width*4.0 || !bolid_visible(mix(tail.xyz,head.xyz,t)) { return vec3<f32>(0.0); }
    let grain_nodes=1.0-floor(optics.w)*0.35*(0.5+0.5*sin(t*37.0+f32(f)*2.7));
    // Same 70%/0.45 s fresh-emission handoff as bolid_model.rs.
    // Old gas returns smoothly to the common cooling train.
    let share=0.7*(1.0-smoothstep(0.35,0.45,elapsed));
    let power=mix(tail.w,head.w,t)*exp(-elapsed/0.45)
        *share*initial_width/wake_width;
    return record_bolid_distance(mix(tail.xyz,head.xyz,t),
        tint*power*1.5*grain_nodes*dilution*exp(-pow(angle/width,2.0)*1.6)
        *bolid_transmission(mix(tail.xyz,head.xyz,t)));
}
fn bolid_train(ray: vec3<f32>, index: u32, i: u32) -> vec3<f32> {
    let color=bolids.events[index].emission.xyz;
    let a=bolids.events[index].trail[i];
    let b=bolids.events[index].trail[i+1u];
    let av=a.xyz-bolid_eye();
    let bv=b.xyz-bolid_eye();
    let da=normalize(av);
    let db=normalize(bv);
    let edge=db-da;
    if dot(edge,edge)<1e-12 { return vec3<f32>(0.0); }
    let along=dot(ray-da,edge)/dot(edge,edge);
    let t=clamp(along,0.0,1.0);
    let angle=length(ray-normalize(mix(da,db,t)));
    let deposited=mix(bolids.events[index].trail_meta[i].x,bolids.events[index].trail_meta[i+1u].x,t);
    let elapsed=max(bolids.events[index].start_age.w-deposited,0.0);
    let corrugation=1.0+0.14*sin(deposited*35.0+bolids.events[index].style.y*19.0);
    let width=mix(bolids.events[index].trail_meta[i].z,bolids.events[index].trail_meta[i+1u].z,t)*corrugation/max(length(mix(av,bv,t)),100.0);
    let filtered_width=max(width,bolid_pixel_angle*0.65);
    if angle>filtered_width*5.0 { return vec3<f32>(0.0); }
    if !bolid_visible(mix(a.xyz,b.xyz,t)) { return vec3<f32>(0.0); }
    // A line spreads in one dimension (a core in two). Filtering must
    // not create energy as a distant trail becomes subpixel-sized.
    let longitudinal=length(edge)*along;
    let gaussian_scale=1.264911064/filtered_width;
    let segment_weight=max(0.0,0.5*(bolid_erf(longitudinal*gaussian_scale)
        +bolid_erf((length(edge)-longitudinal)*gaussian_scale)));
    let transverse=length(ray-normalize(da+edge*along));
    let profile=exp(-pow(transverse/filtered_width,2.0)*1.6)*segment_weight
        *min(width/max(filtered_width,1e-8),1.0);
    let chemistry=mix(bolids.events[index].trail_meta[i].w,bolids.events[index].trail_meta[i+1u].w,t);
    // Day-adapted contrast suppresses both the chemical afterglow and
    // older cooling gas. Applying it only to chemistry left an orange
    // self-luminous line seconds after a daytime head had vanished.
    // Fresh flight remains visible; dust scatters daylight separately.
    let daylight=smoothstep(-0.12,0.04,environment.sky_zenith_rgb_sun_altitude.w);
    let old_gas=smoothstep(0.35,2.0,elapsed);
    let train_contrast=1.0-max(chemistry,old_gas)*daylight*0.998;
    let cooling_color=mix(color,vec3<f32>(0.85,0.55,0.3),min(elapsed/4.0,0.55));
    return record_bolid_distance(mix(a.xyz,b.xyz,t),
        mix(cooling_color,vec3<f32>(0.56,0.72,0.48),chemistry)
        * profile*mix(a.w,b.w,t)*1.5*train_contrast*bolid_transmission(mix(a.xyz,b.xyz,t)));
}
// Dust scatters external daylight; it is not an additive glowing night trail.
@fragment fn fs_dust(input: BolidVertex) -> BolidOutput {
    let uv=bolid_uv(input.clip_position.xy);
    let daylight=smoothstep(-0.23,0.04,environment.sky_zenith_rgb_sun_altitude.w);
    if daylight <= 0.0 { discard; }
    let ndc=uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0);
    let far_point=camera.inverse_sky_view_projection*vec4<f32>(ndc,1.0,1.0);
    let ray=normalize(far_point.xyz/max(abs(far_point.w),1e-6));
    var opacity=0.0;
    var illumination=vec3<f32>(0.0);
    let i=input.part;
    let a=bolids.events[input.event].trail[i].xyz;
    let b=bolids.events[input.event].trail[i+1u].xyz;
    let av=a-bolid_eye();
    let bv=b-bolid_eye();
    let da=normalize(av);
    let db=normalize(bv);
    let edge=db-da;
    if dot(edge,edge)<1e-12 { discard; }
    let along=dot(ray-da,edge)/dot(edge,edge);
    let t=clamp(along,0.0,1.0);
    let deposited=mix(bolids.events[input.event].trail_meta[i].x,bolids.events[input.event].trail_meta[i+1u].x,t);
    let elapsed=max(bolids.events[input.event].start_age.w-deposited,0.0);
    let width=max((80.0+elapsed*18.0)/(atmosphere.config.y*max(length(mix(av,bv,t)),100.0)),0.0005);
    let angle=length(ray-normalize(mix(da,db,t)));
    if angle>width*4.0 || !bolid_visible(mix(a,b,t)) { discard; }
    let density=smoothstep(0.0,2.0,elapsed)*exp(-elapsed/30.0)
        *mix(bolids.events[input.event].trail_meta[i].y,bolids.events[input.event].trail_meta[i+1u].y,t)
        *80.0/(80.0+elapsed*18.0);
    let longitudinal=length(edge)*along;
    let segment_weight=max(0.0,0.5*(bolid_erf(longitudinal/width)
        +bolid_erf((length(edge)-longitudinal)/width)));
    let transverse=length(ray-normalize(da+edge*along));
    let contribution=0.22*density*exp(-pow(transverse/width,2.0))*segment_weight;
    if contribution>opacity {
        opacity=contribution;
        let point=mix(a,b,t);
        // Both output paths use sunlight at the dust altitude. The
        // optical-depth table is available without HDR/compute too;
        // a constant grey fill would illuminate dust in Earth's shadow.
        illumination=physical_sun(point)
            *vec3<f32>(0.65,0.68,0.72)/3.14159265
            *bolid_transmission(point);
    }
    _ = record_bolid_distance(mix(a,b,t),vec3<f32>(contribution));
    // Linear scattered daylight, composed once in the scene output space.
    return BolidOutput(vec4<f32>(tone_map_linear(illumination),opacity*daylight),
        vec4<f32>(bolid_distance_sum,0.0,0.0));
}
