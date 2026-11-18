@group(1) @binding(0) var scene: texture_2d<f32>;
@group(1) @binding(1) var<uniform> params: OutputParams;
@group(1) @binding(2) var<storage,read_write> eye: EyeExposure;

// A horizontal reference at the observer samples the same incident light as
// terrain using the same published shadows. Turning in place only changes the histogram.
fn incident_light_ev() -> f32 {
    let p = atmosphere.eye_relative.xyz;
    let up = atmosphere.up_height.xyz;
    let sun = sunlight_at_surface(p);
    var sun_visibility = 1.0;
    if shadow.flags.y == 1u { sun_visibility = shadow_visibility(p,up,up,sun.direction); }
    let solar = sun.irradiance * max(dot(up, sun.direction), 0.0) * sun_visibility;
    let moon = moonlight_at_surface(p);
    let lunar = moon.irradiance * max(dot(up, moon.direction), 0.0)
        * lunar_shadow_visibility(p,up,up,moon.direction);
    let rgb = solar + lunar + physical_ambient(p);
    return log2(max(dot(rgb, vec3<f32>(0.2126,0.7152,0.0722)), 1e-12));
}

// Half-stop bins span log2 luminance [-32,23.5] in unexposed scene units.
// A stable stratified grid bounds work and includes small bright landscapes.
var<workgroup> histogram: array<atomic<u32>,112>;

fn rank_overlap(begin: f32, end: f32, low: f32, high: f32) -> f32 {
    return max(min(end,high)-max(begin,low),0.0);
}

@compute @workgroup_size(8,8)
fn cs_main(@builtin(local_invocation_id) id: vec3<u32>) {
    let size=vec2<f32>(textureDimensions(scene));
    for (var y=id.y;y<32u;y+=8u) {
        for (var x=id.x;x<32u;x+=8u) {
            let pixel=vec2<i32>((vec2<f32>(f32(x),f32(y))+0.5)*size/32.0);
            let color=textureLoad(scene,pixel,0).rgb;
            let luminance=dot(clamp(color,vec3<f32>(0.0),vec3<f32>(65504.0)),vec3<f32>(0.2126,0.7152,0.0722));
            let ev=log2(max(luminance,1e-18))-params.storage_ev;
            let bin=clamp((ev+32.0)*2.0,0.0,111.0);
            let lower=u32(bin);
            let fraction=u32(round(fract(bin)*16.0));
            atomicAdd(&histogram[lower],16u-fraction);
            atomicAdd(&histogram[min(lower+1u,111u)],fraction);
        }
    }
    workgroupBarrier();
    if any(id.xy!=vec2<u32>(0u)) {return;}

    // A middle-tone band and a separate broad-highlight band keep an illuminated
    // foreground readable against black sky. Tiny stars and the Moon occupy
    // too little area to dominate the upper band.
    var rank=0.0;
    var middle=0.0;
    var highlights=0.0;
    let total=16384.0; // 32 x 32 samples, each with total weight 16.
    for (var i=0u;i<112u;i++) {
        let next=rank+f32(atomicLoad(&histogram[i]))/total;
        let ev=f32(i)*0.5-32.0;
        middle+=ev*rank_overlap(rank,next,0.40,0.80);
        highlights+=ev*rank_overlap(rank,next,0.87,0.97);
        rank=next;
    }
    let upper_luminance_ev=highlights/0.10;
    // In a moonlit scene even diffuse white is below 2^-18 solar units.
    // Give dark soil enough exposure while keeping snow and clouds nocturnal.
    let daylight=smoothstep(-18.0,-12.0,upper_luminance_ev);
    // Retain a difference between a landscape lit by a quarter Moon and
    // the same landscape at full Moon even after the eye has adapted.
    let night_key=mix(0.45,1.0,smoothstep(-28.0,-20.0,upper_luminance_ev));
    let highlight_ev=log2(mix(0.22*night_key,0.55,daylight))-upper_luminance_ev;
    let middle_target=mix(0.055*night_key,0.14,daylight);
    let desired=clamp(min(log2(middle_target)-middle/0.40,highlight_ev),params.minimum_ev,params.maximum_ev);
    let incident=incident_light_ev();
    var adapted=desired;
    if params.elapsed>=0.0 {
        // Follow only the metered change supported by the change in celestial
        // light. Panning from sky to soil keeps real-time eye adaptation.
        let sky_delta=eye.incident_ev-incident;
        // At dusk the solar-altitude ceiling can open faster than the
        // illuminance changes. A target previously on that ceiling must
        // be able to follow it into the newly available exposure range.
        let ceiling_limited=eye.target_ev>=eye.ceiling_ev-0.0001 || desired>=params.maximum_ev-0.0001;
        let ceiling_delta=select(0.0,params.maximum_ev-eye.ceiling_ev,ceiling_limited);
        let measured_delta=desired-eye.target_ev;
        let follow=clamp(measured_delta,min(min(sky_delta,ceiling_delta),0.0),
            max(max(sky_delta,ceiling_delta),0.0))*params.fast_forward;
        let remaining=desired-eye.adapted_ev;
        let predicted=eye.adapted_ev+clamp(follow,min(remaining,0.0),max(remaining,0.0));
        let seconds=select(3.0,0.5,desired<predicted);
        adapted=mix(predicted,desired,1.0-exp(-params.elapsed/seconds));
        // A cut from dark sky to a sunlit landscape can span many stops.
        // Keep at most two stops of highlight overshoot while the eye catches up.
        adapted=min(adapted,max(params.minimum_ev,highlight_ev+2.0));
    }
    eye.adapted_ev=adapted;
    eye.multiplier=exp2(adapted-params.storage_ev);
    eye.incident_ev=incident;
    eye.target_ev=desired;
    eye.ceiling_ev=params.maximum_ev;
}
