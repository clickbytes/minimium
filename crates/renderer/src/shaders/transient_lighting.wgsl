// Local finite emitters; RGB intensity is visible power/(4*pi), never exposure.
struct TransientLight {
    position_radius: vec4<f32>,
    rgb_intensity: vec4<f32>,
    range_lifetime: vec4<f32>,
};
struct TransientLightUniform {
    count: vec4<u32>,
    planet: vec4<f32>,
    mapping: vec4<f32>,
    sources: array<TransientLight,4>,
};
@group(0) @binding(11) var<uniform> transient_lights: TransientLightUniform;

fn transient_physical(point: vec3<f32>) -> vec3<f32> {
    let radial = point-transient_lights.planet.xyz;
    let radius = length(radial);
    let altitude = transient_lights.mapping.x
        +(radius-transient_lights.planet.w)*transient_lights.mapping.y;
    return radial/max(radius,0.001)*(transient_lights.planet.w+altitude);
}
fn transient_direction(index: u32, point: vec3<f32>) -> vec3<f32> {
    let delta=transient_lights.sources[index].position_radius.xyz-point;
    return delta/max(length(delta),0.001);
}
// Irradiance on a plane perpendicular to the source ray, in linear scene units.
fn transient_incident(index: u32, point: vec3<f32>) -> vec3<f32> {
    let source=transient_lights.sources[index];
    if source.range_lifetime.y<=0.0 || source.rgb_intensity.w<=0.0 {return vec3(0.0);}
    let delta=source.position_radius.xyz-point;
    let distance=length(delta);
    let ray=delta/max(distance,0.001);
    let radial=point-transient_lights.planet.xyz;
    let radius=transient_lights.planet.w;
    let b=dot(radial,ray);
    let c=(length(radial)-radius)*(length(radial)+radius);
    if b<0.0 && c<=0.0 {return vec3(0.0);}
    let discriminant=b*b-c;
    if b<0.0 && discriminant>=0.0 {
        let hit=c/max(-b+sqrt(discriminant),0.001);
        if hit>=-0.01 && hit<distance {return vec3(0.0);}
    }
    let physical_delta=transient_physical(source.position_radius.xyz)-transient_physical(point);
    let distance_squared=dot(physical_delta,physical_delta);
    let range=source.range_lifetime.x;
    if distance_squared>=range*range {return vec3(0.0);}
    let fade=1.0-smoothstep(range*0.8,range,sqrt(distance_squared));
    let denominator=max(distance_squared,max(source.position_radius.w*source.position_radius.w,0.0001));
    return source.rgb_intensity.rgb*(source.rgb_intensity.w*fade/denominator)
        *bolid_air_transmission(point+environment.render_origin.xyz,
            source.position_radius.xyz+environment.render_origin.xyz);
}
fn transient_irradiance(point: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    var total=vec3(0.0);
    for (var i=0u; i<min(transient_lights.count.x,4u); i++) {
        total+=transient_incident(i,point)*max(dot(normal,transient_direction(i,point)),0.0);
    }
    return total;
}
fn transient_isotropic(point: vec3<f32>) -> vec3<f32> {
    var total=vec3(0.0);
    for (var i=0u; i<min(transient_lights.count.x,4u); i++) {
        total+=transient_incident(i,point);
    }
    return total;
}
