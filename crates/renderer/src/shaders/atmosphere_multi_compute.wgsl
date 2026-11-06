// Bounded isotropic fill approximation; not the full Hillaire solution.
@compute @workgroup_size(8,8) fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= vec2<u32>(32u)) { return; }
    let uv = vec2<f32>(id.xy)/31.0;
    let h = uv.y*uv.y*atmosphere.radii.y;
    let mu = uv.x*2.0-1.0;
    let sun = vec3<f32>(sqrt(max(1.0-mu*mu,0.0)),mu,0.0);
    let light = air_light(h,vec3<f32>(0.0,1.0,0.0),sun);
    let day = smoothstep(-0.12,0.15,sun.y);
    let result = light*(vec3<f32>(1.0)-light)*0.08+vec3<f32>(0.003,0.006,0.012)*day;
    textureStore(output_s,vec2<i32>(id.xy),vec4<f32>(result,1.0));
}
