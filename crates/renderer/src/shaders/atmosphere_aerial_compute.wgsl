@compute @workgroup_size(4,4,4) fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let edge=select(16u,32u,atmosphere.config.z>0.5);
    if any(id>=vec3<u32>(edge,edge,32u)) {return;}
    let uv=vec2<f32>(id.xy)/f32(edge-1u);
    let distance = exp(f32(id.z)/31.0*log(1.0+atmosphere.eye_relative.w))-1.0;
    let path = integrate_air(atmosphere_ray(uv),distance,select(8u,16u,atmosphere.config.z>0.5));
    textureStore(output_s,vec3<i32>(id),vec4<f32>(min(path.scattering*scene_exposure(),vec3<f32>(65000.0)),path.fog));
    textureStore(output_t,vec3<i32>(id),vec4<f32>(path.transmission,1.0));
}
