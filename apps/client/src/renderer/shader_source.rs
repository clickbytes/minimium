pub(super) fn world() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
        include_str!("sky_common.wgsl"),
        include_str!("shadow_sampling.wgsl"),
        include_str!("weather_field.wgsl"),
        include_str!("cloud_shadow.wgsl"),
        include_str!("world_weather.wgsl"),
        include_str!("world.wgsl"),
    ])
}

pub(super) fn globe() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
        include_str!("sky_common.wgsl"),
        include_str!("weather_field.wgsl"),
        include_str!("cloud_shadow.wgsl"),
        include_str!("atmosphere_common.wgsl"),
        include_str!("globe.wgsl"),
    ])
}

pub(super) fn sky() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
        include_str!("sky_common.wgsl"),
        include_str!("sky.wgsl"),
    ])
}

pub(super) fn sun() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
        include_str!("sky_common.wgsl"),
        include_str!("sun.wgsl"),
    ])
}

pub(super) fn stars() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
        include_str!("sky_common.wgsl"),
        include_str!("stars.wgsl"),
    ])
}

pub(super) fn moon() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
        include_str!("sky_common.wgsl"),
        include_str!("moon.wgsl"),
    ])
}

pub(super) fn moon_sphere() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
        include_str!("sky_common.wgsl"),
        include_str!("moon_sphere.wgsl"),
    ])
}

pub(super) fn celestial_halo() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
        include_str!("sky_common.wgsl"),
        include_str!("celestial_halo.wgsl"),
    ])
}

pub(super) fn clouds(aggregates: bool) -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
        include_str!("sky_common.wgsl"),
        include_str!("weather_frame.wgsl"),
        include_str!("weather_field.wgsl"),
        include_str!("cloud_shadow.wgsl"),
        include_str!("atmosphere_common.wgsl"),
        include_str!("cloud_lighting.wgsl"),
        include_str!("cloud_detail.wgsl"),
        include_str!("cloud_density.wgsl"),
        include_str!("cloud_intervals.wgsl"),
        if aggregates {
            include_str!("clouds.wgsl")
        } else {
            include_str!("cloud_ellipsoids.wgsl")
        },
    ])
}

pub(super) fn cloud_composite() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("weather_frame.wgsl"),
        include_str!("cloud_composite.wgsl"),
    ])
}

pub(super) fn atmosphere() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
        include_str!("sky_common.wgsl"),
        include_str!("weather_frame.wgsl"),
        include_str!("atmosphere_common.wgsl"),
        include_str!("atmosphere.wgsl"),
    ])
}

pub(super) fn weather_fog() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
        include_str!("sky_common.wgsl"),
        include_str!("weather_frame.wgsl"),
        include_str!("weather_field.wgsl"),
        include_str!("cloud_shadow.wgsl"),
        include_str!("weather_fog.wgsl"),
    ])
}

pub(super) fn rain() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("weather_frame.wgsl"),
        include_str!("weather_field.wgsl"),
        include_str!("cloud_shadow.wgsl"),
        include_str!("rain.wgsl"),
    ])
}

pub(super) fn bolt() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("weather_frame.wgsl"),
        include_str!("bolt.wgsl"),
    ])
}

pub(super) fn marker() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
        include_str!("sky_common.wgsl"),
        include_str!("marker.wgsl"),
    ])
}

pub(super) fn ui() -> String {
    combine(&[include_str!("output_color.wgsl"), include_str!("ui.wgsl")])
}

pub(super) fn encode_output() -> String {
    combine(&[
        include_str!("output_color.wgsl"),
        include_str!("encode_output.wgsl"),
    ])
}

pub(super) fn ui_output(surface_is_srgb: bool) -> String {
    combine(&[
        include_str!("output_color.wgsl"),
        &include_str!("ui_output.wgsl").replace(
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
