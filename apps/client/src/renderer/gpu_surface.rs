pub(super) fn create_depth_view(
    device: &wgpu::Device,
    size: winit::dpi::PhysicalSize<u32>,
) -> wgpu::TextureView {
    minimium_renderer::ui_pipeline::create_depth_view(device, [size.width, size.height])
}

pub(super) fn is_srgb(format: wgpu::TextureFormat) -> bool {
    minimium_renderer::ui_pipeline::is_srgb(format)
}
