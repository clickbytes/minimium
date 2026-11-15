// Finite angular, single-scattering approximation. No dynamic atmosphere LUT.
const METEOR_HALO_SIGMA_MAX: f32 = 0.006;
fn meteor_optical_active(optics: vec4<f32>) -> bool {
    return optics.z>0.5 && u32(optics.z)<=transient_lights.count.x;
}
fn meteor_halo(point: vec3<f32>, radius: f32, radiance: vec3<f32>,
    angle: f32, transmission: vec3<f32>, pixel_ray: vec3<f32>) -> vec3<f32> {
    let eye=camera.eye_world.xyz-environment.render_origin.xyz;
    let delta=point-eye;
    let distance=length(delta);
    let radial=eye-transient_lights.planet.xyz;
    let planet=transient_lights.planet.w;
    let b=dot(radial,pixel_ray);
    let c=(length(radial)-planet)*(length(radial)+planet);
    let discriminant=b*b-c;
    if b<0.0 && discriminant>=0.0 {
        let hit=c/max(-b+sqrt(discriminant),0.001);
        if hit>=0.0 && hit<distance {return vec3(0.0);}
    }
    let physical_distance=length(transient_physical(point)-transient_physical(eye));
    let flux=radiance*(3.14159265*radius*radius/max(physical_distance*physical_distance,1.0));
    let brightness=dot(flux,vec3(0.2126,0.7152,0.0722));
    let gate=smoothstep(0.0001,0.02,brightness);
    let mie=max(atmosphere.mie.x,0.0);
    let molecular=max(dot(atmosphere.rayleigh.xyz,vec3(0.2126,0.7152,0.0722)),1e-8);
    let haze=mie/(mie+molecular);
    let ray=delta/max(distance,0.001);
    let closest=eye+ray*clamp(-dot(radial,ray),0.0,distance);
    let height=max(length(transient_physical(closest))-planet,0.0);
    let density=exp(-height/max(atmosphere.radii.w,1000.0));
    var sigma=mix(0.0015,METEOR_HALO_SIGMA_MAX,haze*density);
    var fraction=(1.0-transmission)*(0.08*haze*density);
    if !physical_atmosphere() {
        sigma=0.0015;
        fraction=(1.0-transmission)*0.015;
    }
    let kernel=exp(-angle*angle/(sigma*sigma))/(3.14159265*sigma*sigma);
    return flux*fraction*(gate*kernel);
}
