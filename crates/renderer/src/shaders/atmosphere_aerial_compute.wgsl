// Each invocation integrates one view ray front to back. Each slice stores the
// prefix needed by all opaque depths, transparent objects and the sky join.
@compute @workgroup_size(8,8) fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size=aerial_size();
    if any(id.xy>=size.xy) {return;}
    let uv=vec2<f32>(id.xy)/vec2<f32>(size.xy-1u);
    let ray=atmosphere_ray(uv);
    var path=clear_air_path();
    var previous=0.0;
    for (var z=0u;z<size.z;z++) {
        let distance=aerial_distance(f32(z)/f32(size.z-1u));
        if z>0u {
            let interval=integrate_air_interval(ray,previous,distance,2u);
            path=join_air_paths(path,interval);
        }
        let position=vec3<i32>(vec3<u32>(id.xy,z));
        textureStore(output_s,position,vec4<f32>(min(path.scattering*scene_exposure(),vec3<f32>(65000.0)),path.fog));
        textureStore(output_t,position,vec4<f32>(path.transmission,path.shadowed_fog));
        previous=distance;
    }
}
