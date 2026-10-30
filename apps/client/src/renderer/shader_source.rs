pub(super) fn world() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
        include_str!("sky_common.wgsl"),
        include_str!("world.wgsl"),
    ])
}

pub(super) fn globe() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
        include_str!("sky_common.wgsl"),
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

pub(super) fn marker() -> String {
    combine(&[
        include_str!("frame.wgsl"),
        include_str!("output_color.wgsl"),
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

fn combine(parts: &[&str]) -> String {
    let capacity = parts.iter().map(|part| part.len() + 1).sum();
    let mut source = String::with_capacity(capacity);
    for part in parts {
        source.push_str(part);
        source.push('\n');
    }
    source
}
