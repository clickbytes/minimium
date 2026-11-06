// All cover fractions contribute mean color. Only two dominant layers add detail.
// Detail vanishes before the selected pair changes, including three-way ties.
fn blend_terrain(high: vec3<f32>, low: vec3<f32>, n: vec3<f32>,
    source_weights: array<f32,6>, dx: vec3<f32>, dy: vec3<f32>,
    detail: f32, greenness: f32) -> MaterialSample {
    var weights = source_weights;
    var total = 0.0;
    for (var i = 0u; i < 6u; i++) { total += weights[i]; }
    for (var i = 0u; i < 6u; i++) { weights[i] /= max(total,0.00001); }
    var first = 0u;
    var second = 1u;
    if weights[second] > weights[first] { first = 1u; second = 0u; }
    for (var i = 2u; i < 6u; i++) {
        if weights[i] > weights[first] { second = first; first = i; }
        else if weights[i] > weights[second] { second = i; }
    }
    var third_weight = 0.0;
    for (var i = 0u; i < 6u; i++) {
        if i != first && i != second { third_weight = max(third_weight,weights[i]); }
    }
    let mean_packed = vec4<f32>(0.85,1.0,0.5,0.04);
    var result = MaterialSample(vec3<f32>(0.0),n,mean_packed);
    var perturbation = vec3<f32>(0.0);
    var first_sample = MaterialSample(vec3<f32>(0.0),n,mean_packed);
    var second_sample = first_sample;
    var first_gate = 0.0;
    var second_gate = 0.0;
    for (var i = 0u; i < 6u; i++) {
        if weights[i] > 0.0 {
            var mean = terrain.mean_albedo[i].rgb;
            if i == 2u { mean = grass_color(mean,greenness); }
            result.albedo += mean*weights[i];
            let resolved = material_resolution(i,dx,dy);
            if (i == first || i == second) && resolved > 0.0 {
                let gate = smoothstep(0.0,max(0.03,weights[first]*0.25),weights[i]-third_weight);
                if gate > 0.0 {
                    var sample = terrain_material(high,low,n,i,dx,dy,detail);
                    if i == 2u { sample.albedo = grass_color(sample.albedo,greenness); }
                    // Microheight modulates resolved detail, without changing pair continuity.
                    let relief = mix(1.0,0.5+0.5*sample.packed.b,1.0-terrain.scales_b_blend.z);
                    let fraction = weights[i]*gate*relief*resolved;
                    if i == first {
                        first_sample = sample;
                        first_gate = gate*resolved;
                    } else {
                        second_sample = sample;
                        second_gate = gate*resolved;
                    }
                    result.albedo += (sample.albedo-mean)*fraction;
                    result.packed += (sample.packed-mean_packed)*fraction;
                    perturbation += (sample.normal-n)*fraction;
                }
            }
        }
    }
    // Height changes the boundary between the resolved pair. All omitted layers
    // retain their mean contribution; pair changes fade before any transfer.
    let mass = weights[first]+weights[second];
    let first_share = weights[first]/max(mass,0.00001);
    let height_bias = (first_sample.packed.b-second_sample.packed.b)
        *(1.0-terrain.scales_b_blend.z)*0.5;
    let transfer = (clamp(first_share+height_bias,0.0,1.0)*mass-weights[first])
        *min(first_gate,second_gate);
    result.albedo = max(result.albedo+(first_sample.albedo-second_sample.albedo)*transfer,vec3<f32>(0.0));
    result.packed += (first_sample.packed-second_sample.packed)*transfer;
    perturbation += (first_sample.normal-second_sample.normal)*transfer;
    result.normal = normalize(n+perturbation);
    return result;
}

fn grass_color(color: vec3<f32>, greenness: f32) -> vec3<f32> {
    let luminance = dot(color,vec3<f32>(0.2126,0.7152,0.0722));
    return mix(luminance*vec3<f32>(1.2,1.03,0.58),color,greenness);
}
