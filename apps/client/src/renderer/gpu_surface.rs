pub(super) fn create_depth_view(
    device: &wgpu::Device,
    size: winit::dpi::PhysicalSize<u32>,
) -> wgpu::TextureView {
    minimium_renderer::ui_pipeline::create_depth_view(device, [size.width, size.height])
}

pub(super) fn is_srgb(format: wgpu::TextureFormat) -> bool {
    minimium_renderer::ui_pipeline::is_srgb(format)
}

pub(super) fn emit_gpu_event(
    logger: &minimium_diagnostics::DomainLogger,
    code: &'static str,
    fields: &[(&str, String)],
) {
    use minimium_diagnostics::{EventCode, Level};
    let Ok(code) = EventCode::new(code) else {
        return;
    };
    let mut event = logger.event(Level::Info, code);
    for (key, value) in fields {
        event = event.field(key, value);
    }
    logger.emit(event);
}
