@group(1) @binding(0) var scene_depth: texture_depth_2d;
@group(1) @binding(1) var scene_color: texture_2d<f32>;
@vertex fn vs_main(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {
    let points = array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
    return vec4<f32>(points[vertex],0.0,1.0);
}
@fragment fn fs_atmosphere(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(p.xy);
    let color = textureLoad(scene_color,pixel,0).rgb;
    if !physical_atmosphere() { return vec4<f32>(color,1.0); }
    let uv = p.xy/camera.viewport.xy;
    let depth = textureLoad(scene_depth,pixel,0);
    var path: AirPath;
    if depth >= 1.0 {
        path = sample_sky(uv);
    } else {
        let local = atmosphere.inverse_view_projection*vec4<f32>(uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),depth,1.0);
        path = sample_aerial(local.xyz/max(abs(local.w),1e-8));
    }
    if atmosphere.weather.w == 1.0 { return vec4<f32>(path.transmission,1.0); }
    if atmosphere.weather.w == 2.0 { return vec4<f32>(path.scattering,1.0); }
    if atmosphere.weather.w == 3.0 {return vec4<f32>(clamp(-log(max(path.transmission,vec3<f32>(1e-6)))/12.0,vec3<f32>(0.0),vec3<f32>(1.0)),1.0);}
    if atmosphere.weather.w == 4.0 {
        var distance=1.0;
        if depth<1.0 {
            let local=atmosphere.inverse_view_projection*vec4<f32>(uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),depth,1.0);
            distance=log(1.0+length(local.xyz/max(abs(local.w),1e-8)-atmosphere.eye_relative.xyz)*atmosphere.config.y)/log(1.0+atmosphere.eye_relative.w);
        }
        return vec4<f32>(vec3<f32>(clamp(distance,0.0,1.0)),1.0);
    }
    if atmosphere.weather.w == 5.0 {return vec4<f32>(vec3<f32>(1.0-exp(-path.fog)),1.0);}
    return vec4<f32>(color*path.transmission+path.scattering*scene_exposure(),1.0);
}
