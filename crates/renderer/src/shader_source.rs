pub fn clouds() -> String {
    combine(&[
        include_str!("shaders/frame.wgsl"),
        include_str!("shaders/atmosphere_frame.wgsl"),
        include_str!("shaders/atmosphere_common.wgsl"),
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/sky_common.wgsl"),
        include_str!("shaders/transient_lighting.wgsl"),
        include_str!("shaders/clouds.wgsl"),
    ])
}

pub fn meteors() -> String {
    combine(&[
        include_str!("shaders/frame.wgsl"),
        include_str!("shaders/atmosphere_frame.wgsl"),
        include_str!("shaders/atmosphere_common.wgsl"),
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/sky_common.wgsl"),
        include_str!("shaders/transient_lighting.wgsl"),
        include_str!("shaders/meteor_optics.wgsl"),
        include_str!("shaders/meteors.wgsl"),
    ])
}

pub fn cloud_composite() -> String {
    include_str!("shaders/cloud_composite.wgsl").into()
}

pub fn world() -> String {
    combine(&[
        include_str!("shaders/frame.wgsl"),
        include_str!("shaders/atmosphere_frame.wgsl"),
        include_str!("shaders/atmosphere_common.wgsl"),
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/sky_common.wgsl"),
        include_str!("shaders/transient_lighting.wgsl"),
        include_str!("shaders/shadow_sampling.wgsl"),
        include_str!("shaders/world.wgsl"),
    ])
}

pub fn terrain() -> String {
    combine(&[
        include_str!("shaders/frame.wgsl"),
        include_str!("shaders/atmosphere_frame.wgsl"),
        include_str!("shaders/atmosphere_common.wgsl"),
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/sky_common.wgsl"),
        include_str!("shaders/transient_lighting.wgsl"),
        include_str!("shaders/shadow_sampling.wgsl"),
        include_str!("shaders/terrain_material.wgsl"),
        include_str!("shaders/terrain_patch.wgsl"),
        include_str!("shaders/terrain_blend.wgsl"),
        include_str!("shaders/terrain_climate.wgsl"),
        include_str!("shaders/terrain.wgsl"),
    ])
}

pub fn sky() -> String {
    combine(&[
        include_str!("shaders/frame.wgsl"),
        include_str!("shaders/atmosphere_frame.wgsl"),
        include_str!("shaders/atmosphere_common.wgsl"),
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/sky_common.wgsl"),
        include_str!("shaders/sky.wgsl"),
    ])
}

pub fn ocean() -> String {
    combine(&[
        include_str!("shaders/frame.wgsl"),
        include_str!("shaders/atmosphere_frame.wgsl"),
        include_str!("shaders/atmosphere_common.wgsl"),
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/sky_common.wgsl"),
        include_str!("shaders/transient_lighting.wgsl"),
        include_str!("shaders/shadow_sampling.wgsl"),
        include_str!("shaders/terrain_material.wgsl"),
        include_str!("shaders/ocean.wgsl"),
    ])
}

pub fn sun() -> String {
    combine(&[
        include_str!("shaders/frame.wgsl"),
        include_str!("shaders/atmosphere_frame.wgsl"),
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/sky_common.wgsl"),
        include_str!("shaders/sun.wgsl"),
    ])
}

pub fn stars() -> String {
    combine(&[
        include_str!("shaders/frame.wgsl"),
        include_str!("shaders/atmosphere_frame.wgsl"),
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/sky_common.wgsl"),
        include_str!("shaders/stars.wgsl"),
    ])
}

pub fn moon() -> String {
    combine(&[
        include_str!("shaders/frame.wgsl"),
        include_str!("shaders/atmosphere_frame.wgsl"),
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/sky_common.wgsl"),
        include_str!("shaders/moon.wgsl"),
    ])
}

pub fn moon_sphere() -> String {
    combine(&[
        include_str!("shaders/frame.wgsl"),
        include_str!("shaders/atmosphere_frame.wgsl"),
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/sky_common.wgsl"),
        include_str!("shaders/moon_sphere.wgsl"),
    ])
}

pub fn celestial_halo() -> String {
    combine(&[
        include_str!("shaders/frame.wgsl"),
        include_str!("shaders/atmosphere_frame.wgsl"),
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/sky_common.wgsl"),
        include_str!("shaders/celestial_halo.wgsl"),
    ])
}

pub fn atmosphere() -> String {
    combine(&[
        include_str!("shaders/frame.wgsl"),
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/sky_common.wgsl"),
        include_str!("shaders/atmosphere_frame.wgsl"),
        include_str!("shaders/atmosphere_common.wgsl"),
        include_str!("shaders/atmosphere.wgsl"),
    ])
}

pub fn marker() -> String {
    combine(&[
        include_str!("shaders/frame.wgsl"),
        include_str!("shaders/atmosphere_frame.wgsl"),
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/sky_common.wgsl"),
        include_str!("shaders/marker.wgsl"),
    ])
}

pub fn ui() -> String {
    combine(&[
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/ui.wgsl"),
    ])
}

pub fn encode_output() -> String {
    combine(&[
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/encode_output.wgsl"),
    ])
}

pub fn ui_output(surface_is_srgb: bool) -> String {
    combine(&[
        include_str!("shaders/output_color.wgsl"),
        &include_str!("shaders/ui_output.wgsl").replace(
            "OUTPUT_SURFACE_IS_SRGB",
            if surface_is_srgb { "true" } else { "false" },
        ),
    ])
}

fn combine(parts: &[&str]) -> String {
    let capacity = parts.iter().map(|part| part.len() + 1).sum();
    let mut source = String::with_capacity(capacity);
    for part in parts {
        source.push_str(part);
        source.push('\n');
    }
    source
}

pub fn atmosphere_compute(kind: &str) -> String {
    let (dimension, entry) = match kind {
        "sky" => ("2d", include_str!("shaders/atmosphere_sky_compute.wgsl")),
        "aerial" => ("3d", include_str!("shaders/atmosphere_aerial_compute.wgsl")),
        _ => ("2d", include_str!("shaders/atmosphere_multi_compute.wgsl")),
    };
    let body = include_str!("shaders/atmosphere_compute.wgsl")
        .replace("__DIM__", dimension)
        .replace("__ENTRY__", entry);
    combine(&[
        include_str!("shaders/frame.wgsl"),
        include_str!("shaders/atmosphere_frame.wgsl"),
        include_str!("shaders/atmosphere_common.wgsl"),
        &body,
    ])
}

pub fn bolid_composite() -> String {
    combine(&[
        include_str!("shaders/output_color.wgsl"),
        include_str!("shaders/bolid_composite.wgsl"),
    ])
}
