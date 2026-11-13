// Each invocation shades one core or line, never every event at every pixel.
// Counts match bolid_dust.rs: 20 cores, 20 wakes, 23 train segments per event.
struct BolidVertex {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) @interpolate(flat) event: u32,
    @location(1) @interpolate(flat) part: u32,
};

fn bolid_quad(vertex: u32, instance: u32, dust: bool) -> BolidVertex {
    let stride=select(63u,23u,dust);
    let event=instance/stride;
    let part=instance%stride;
    var a=bolids.events[event].head_power;
    var b=a;
    var support=0.0;
    var enabled=bolids.events[event].start_age.w>=0.0;
    let scale=max(atmosphere.config.y,0.001);
    if dust {
        a=bolids.events[event].trail[part];
        b=bolids.events[event].trail[part+1u];
        let elapsed=max(bolids.events[event].start_age.w-min(bolids.events[event].trail_meta[part].x,bolids.events[event].trail_meta[part+1u].x),0.0);
        support=4.0*(80.0+elapsed*18.0)/scale;
        enabled=enabled && bolids.events[event].style.w>0.0
            && max(bolids.events[event].trail_meta[part].y,bolids.events[event].trail_meta[part+1u].y)>0.00001;
    } else if part<40u {
        let slot=part%20u;
        if slot>0u {a=bolids.events[event].fragments[slot-1u];}
        b=a;
        support=max(bolids.events[event].optics[slot].x,2.0/scale)*9.0;
        if part>=20u {
            b=bolids.events[event].tails[slot];
            support=(max(bolids.events[event].optics[slot].x,8.0/scale)+2.0/scale)*4.0;
        }
        enabled=enabled && max(a.w,b.w)>0.000001;
    } else {
        let slot=part-40u;
        a=bolids.events[event].trail[slot];
        b=bolids.events[event].trail[slot+1u];
        support=max(bolids.events[event].trail_meta[slot].z,bolids.events[event].trail_meta[slot+1u].z)*5.7;
        enabled=enabled && max(a.w,b.w)>0.000001;
    }
    if dust || part>=20u {enabled=enabled && distance(a.xyz,b.xyz)>0.00001;}
    var av=a.xyz-bolid_eye();
    var bv=b.xyz-bolid_eye();
    let aw=(camera.sky_view_projection*vec4<f32>(av,1.0)).w;
    let bw=(camera.sky_view_projection*vec4<f32>(bv,1.0)).w;
    enabled=enabled && (aw>0.01 || bw>0.01);
    // Clip ribbons at the eye plane, retaining their visible half in orbit.
    if aw<0.01 && bw>=0.01 {av=mix(av,bv,(0.01-aw)/(bw-aw));}
    if bw<0.01 && aw>=0.01 {bv=mix(bv,av,(0.01-bw)/(aw-bw));}
    let ca=camera.sky_view_projection*vec4<f32>(av,1.0);
    let cb=camera.sky_view_projection*vec4<f32>(bv,1.0);
    let pa=ca.xy/max(ca.w,0.001);
    let pb=cb.xy/max(cb.w,0.001);
    let norm=max(length(vec3(camera.sky_view_projection[0].x,camera.sky_view_projection[1].x,camera.sky_view_projection[2].x)),
        length(vec3(camera.sky_view_projection[0].y,camera.sky_view_projection[1].y,camera.sky_view_projection[2].y)));
    let depth=max(min(ca.w,cb.w),0.001);
    let obliquity=max(max(length(av),length(bv))/depth,1.0);
    var margin=vec2<f32>(support*norm/depth*obliquity)
        +24.0/vec2<f32>(bolids.count.yz)*obliquity;
    if dust {margin+=vec2<f32>(0.002*norm*obliquity*obliquity);}
    let lo=max(min(pa,pb)-margin,vec2<f32>(-1.0));
    let hi=min(max(pa,pb)+margin,vec2<f32>(1.0));
    let corners=array<vec2<f32>,6>(vec2(0,0),vec2(1,0),vec2(1,1),vec2(0,0),vec2(1,1),vec2(0,1));
    var p=mix(lo,hi,corners[vertex]);
    if !enabled || any(hi<lo) {p=vec2<f32>(2.0);}
    return BolidVertex(vec4<f32>(p,0.0,1.0),event,part);
}

@vertex fn vs_bolid(@builtin(vertex_index) vertex:u32, @builtin(instance_index) instance:u32) -> BolidVertex {
    return bolid_quad(vertex,instance,false);
}
@vertex fn vs_bolid_dust(@builtin(vertex_index) vertex:u32, @builtin(instance_index) instance:u32) -> BolidVertex {
    return bolid_quad(vertex,instance,true);
}
