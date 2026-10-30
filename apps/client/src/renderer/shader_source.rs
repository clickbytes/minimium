pub(super) fn world() -> String {
    combine(include_str!("frame.wgsl"), include_str!("world.wgsl"))
}

pub(super) fn globe() -> String {
    combine(include_str!("frame.wgsl"), include_str!("globe.wgsl"))
}

fn combine(common: &str, shader: &str) -> String {
    let mut source = String::with_capacity(common.len() + shader.len() + 1);
    source.push_str(common);
    source.push('\n');
    source.push_str(shader);
    source
}
