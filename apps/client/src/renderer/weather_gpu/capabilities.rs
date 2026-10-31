pub(in crate::renderer) struct CloudCapabilities {
    pub(in crate::renderer) format: wgpu::TextureFormat,
    pub(in crate::renderer) precise: bool,
    pub(in crate::renderer) adapter: String,
}

impl CloudCapabilities {
    pub(in crate::renderer) fn new(adapter: &wgpu::Adapter) -> Self {
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let features = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba16Float);
        let limits = adapter.limits();
        let precise = features.allowed_usages.contains(usage)
            && features
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
            && limits.max_color_attachment_bytes_per_sample >= 32;
        Self {
            format: if precise {
                wgpu::TextureFormat::Rgba16Float
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
            precise,
            adapter: adapter.get_info().name,
        }
    }
    pub(super) fn optical_scale(&self) -> f32 {
        if self.precise {
            1.0
        } else {
            1.0 / 32.0
        }
    }
}
