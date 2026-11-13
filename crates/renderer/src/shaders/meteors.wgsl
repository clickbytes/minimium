@group(1) @binding(0) var scene_depth: texture_depth_2d;
struct MeteorVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) a: vec4<f32>,
    @location(2) @interpolate(flat) b: vec4<f32>,
    @location(3) @interpolate(flat) color: vec4<f32>,
    @location(4) @interpolate(flat) optics: vec4<f32>,
};
@vertex fn vs_main(@builtin(vertex_index) index:u32,
    @location(0) start:vec4<f32>, @location(1) end:vec4<f32>,
    @location(2) color:vec4<f32>, @location(3) optics:vec4<f32>)->MeteorVertex {
    var a=start;
    var b=end;
    let start_w=(camera.view_projection*vec4(start.xyz,1.0)).w;
    let end_w=(camera.view_projection*vec4(end.xyz,1.0)).w;
    // Clip crossing ribbons rather than discarding their visible half.
    if start_w<0.01 && end_w>=0.01 {
        a=mix(start,end,(0.01-start_w)/(end_w-start_w));
    }
    if end_w<0.01 && start_w>=0.01 {
        b=mix(end,start,(0.01-end_w)/(start_w-end_w));
    }
    let ca=camera.view_projection*vec4(a.xyz,1.0);
    let cb=camera.view_projection*vec4(b.xyz,1.0);
    let pa=ca.xy/max(ca.w,0.001);
    let pb=cb.xy/max(cb.w,0.001);
    let radius=max(a.w,b.w);
    // Conservative projection norm, independent of camera orientation.
    let norm=max(length(vec3(camera.view_projection[0].x,camera.view_projection[1].x,camera.view_projection[2].x)),
        length(vec3(camera.view_projection[0].y,camera.view_projection[1].y,camera.view_projection[2].y)));
    let margin=vec2(max(radius*norm/min(max(ca.w,0.001),max(cb.w,0.001))*4.0,0.0))
        +32.0/camera.viewport.xy;
    let corners=array<vec2<f32>,6>(vec2(0,0),vec2(1,0),vec2(1,1),vec2(0,0),vec2(1,1),vec2(0,1));
    var p=mix(min(pa,pb)-margin,max(pa,pb)+margin,corners[index]);
    if start_w<0.01 && end_w<0.01 {p=vec2(2.0);}
    return MeteorVertex(vec4(p,0.0,1.0),p*vec2(0.5,-0.5)+0.5,a,b,color,optics);
}
struct MeteorOutput {
    @location(0) emission:vec4<f32>,
    @location(1) distance:vec4<f32>,
};
@fragment fn fs_main(input:MeteorVertex)->MeteorOutput {
    return shade_meteor(input,false);
}
@fragment fn fs_dust(input:MeteorVertex)->MeteorOutput {
    return shade_meteor(input,true);
}
fn shade_meteor(input:MeteorVertex,dust:bool)->MeteorOutput {
    let ca=camera.view_projection*vec4(input.a.xyz,1.0);
    let cb=camera.view_projection*vec4(input.b.xyz,1.0);
    let pa=(ca.xy/ca.w*vec2(0.5,-0.5)+0.5)*camera.viewport.xy;
    let pb=(cb.xy/cb.w*vec2(0.5,-0.5)+0.5)*camera.viewport.xy;
    let pixel=input.uv*camera.viewport.xy;
    let footprint=max(length(dpdx(pixel)),length(dpdy(pixel)));
    if (input.optics.x>1.5)!=dust || input.color.w==0.0 {discard;}
    let edge=pb-pa;
    let screen_t=clamp(dot(pixel-pa,edge)/max(dot(edge,edge),1e-6),0.0,1.0);
    let t=(screen_t/cb.w)/max((1.0-screen_t)/ca.w+screen_t/cb.w,1e-8);
    let point=mix(input.a.xyz,input.b.xyz,t);
    let clip=camera.view_projection*vec4(point,1.0);
    let stored=textureLoad(scene_depth,vec2<i32>(clamp(pixel,vec2(0.0),camera.viewport.xy-1.0)),0);
    if stored<1.0 && clip.z/clip.w>stored+0.0000002 {discard;}
    let eye=camera.eye_world.xyz;
    let local_eye=eye-environment.render_origin.xyz;
    let delta=point-local_eye;
    let distance=length(delta);
    let ray=delta/max(distance,0.001);
    let radius=atmosphere.config.x;
    let b=dot(eye,ray);
    let c=(length(eye)-radius)*(length(eye)+radius);
    let d=b*b-c;
    if b<0.0 && d>=0.0 {
        let hit=c/max(-b+sqrt(d),0.001);
        if hit>0.0 && hit<distance {discard;}
    }
    let projected_radius=mix(input.a.w,input.b.w,t)
        *length(vec3(camera.view_projection[0].y,camera.view_projection[1].y,camera.view_projection[2].y))
        *camera.viewport.y*0.5/max(clip.w,0.001);
    let width=max(projected_radius,0.7*footprint);
    let offset=length(pixel-mix(pa,pb,screen_t))/width;
    if offset>4.0 {discard;}
    let footprint_gain=min(projected_radius/width,1.0);
    let core=dot(edge,edge)<1e-6;
    let coverage=exp(-offset*offset)*select(footprint_gain,footprint_gain*footprint_gain,core);
    var transmission=bolid_air_transmission(eye,point+environment.render_origin.xyz);
    if physical_atmosphere() {transmission=sample_aerial(point).transmission;}
    if dust {
        let daylight=smoothstep(-0.18,0.03,environment.sky_zenith_rgb_sun_altitude.w);
        let opacity=clamp(input.color.w*coverage*daylight,0.0,0.12);
        if opacity<0.00001 {discard;}
        let scattered=tone_map_linear(physical_sun(point)*input.color.rgb*transmission/3.14159265);
        let weight=opacity;
        return MeteorOutput(vec4(scattered*opacity,opacity),vec4(distance/1000000.0*weight,weight,0.0,0.0));
    }
    let gradient=mix(2.0-input.optics.y,input.optics.y,t);
    var contrast=1.0;
    if input.optics.x>0.5 {
        contrast=1.0-smoothstep(-0.18,0.03,environment.sky_zenith_rgb_sun_altitude.w);
    }
    let power=input.color.rgb*input.color.w*coverage*gradient*contrast;
    var exposure=environment.ambient_rgb_exposure.w;
    if environment.flags.w!=0u {exposure=scene_exposure();}
    let emitted=min(power*transmission*exposure,vec3(65000.0));
    let weight=min(dot(emitted,vec3(0.2126,0.7152,0.0722)),100.0);
    return MeteorOutput(vec4(emitted,0.0),vec4(distance/1000000.0*weight,weight,0.0,0.0));
}
