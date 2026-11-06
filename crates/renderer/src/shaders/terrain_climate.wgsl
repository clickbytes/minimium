// Both views sample the same UTC endpoints and planet-anchored weather fields.
struct SurfaceWeather {
    cover: vec4<f32>,
    temperature: f32,
};

fn history_sample(map: texture_2d_array<f32>, uv: vec2<f32>, elevation: f32) -> SurfaceWeather {
    let phase = terrain.cover.x;
    let cover = mix(textureSampleLevel(map, material_sampler, uv, 0, 0.0),
        textureSampleLevel(map, material_sampler, uv, 2, 0.0), phase);
    let climate = mix(textureSampleLevel(map, material_sampler, uv, 1, 0.0),
        textureSampleLevel(map, material_sampler, uv, 3, 0.0), phase);
    let temperature = climate.x-(max(elevation,0.0)-max(climate.y,0.0))*0.0065;
    return SurfaceWeather(cover,temperature);
}

fn surface_weather(direction: vec3<f32>, elevation: f32, fallback: vec2<f32>) -> SurfaceWeather {
    var result = SurfaceWeather(vec4<f32>(smoothstep(-3.0,15.0,fallback.x)
        *smoothstep(0.08,0.55,fallback.y),0.0,0.0,fallback.y),fallback.x);
    let longitude = atan2(direction.z,direction.x);
    let latitude = asin(clamp(direction.y,-1.0,1.0));
    var local_influence = 0.0;
    var local_sample = result;
    if terrain.cover_center.w > 0.0 && abs(latitude) < 1.54 {
        let delta = longitude-terrain.cover_center.x;
        let dx = atan2(sin(delta),cos(delta));
        let local = vec2<f32>(dx,latitude-terrain.cover_center.y)/terrain.cover_center.w;
        let edge = max(abs(local.x*2.0-1.0),abs(local.y*2.0-1.0));
        let influence = (1.0-smoothstep(0.65,1.0,edge))*terrain.cover.y;
        local_influence = influence;
        if influence > 0.0 {
            let size = vec2<f32>(textureDimensions(surface_cover));
            let uv = (clamp(local,vec2<f32>(0.0),vec2<f32>(1.0))*(size-1.0)+0.5)/size;
            local_sample = history_sample(surface_cover,uv,elevation);
        }
    }
    if terrain.cover.z > 0.0 && local_influence < 1.0 {
        let size = vec2<f32>(textureDimensions(planet_cover));
        let global_uv = vec2<f32>(longitude/6.28318530718+0.5,0.5-latitude/3.14159265359);
        let uv = (global_uv*(size-1.0)+0.5)/size;
        let sample = history_sample(planet_cover,uv,elevation);
        result.cover = mix(result.cover,sample.cover,terrain.cover.y);
        result.temperature = mix(result.temperature,sample.temperature,terrain.cover.y);
    }
    result.cover = mix(result.cover,local_sample.cover,local_influence);
    result.temperature = mix(result.temperature,local_sample.temperature,local_influence);
    return result;
}
