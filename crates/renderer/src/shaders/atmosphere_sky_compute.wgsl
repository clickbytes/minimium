@compute @workgroup_size(8,8) fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size=select(vec2<u32>(128u,64u),vec2<u32>(192u,108u),atmosphere.config.z>0.5);
    if any(id.xy>=size) {return;}
    let uv=vec2<f32>(id.xy)/vec2<f32>(size-1u);
    let path = integrate_air(atmosphere_ray(uv),1e9,select(12u,24u,atmosphere.config.z>0.5));
    textureStore(output_s,vec2<i32>(id.xy),vec4<f32>(min(path.scattering*scene_exposure(),vec3<f32>(65000.0)),path.fog));
    textureStore(output_t,vec2<i32>(id.xy),vec4<f32>(path.transmission,1.0));
}
