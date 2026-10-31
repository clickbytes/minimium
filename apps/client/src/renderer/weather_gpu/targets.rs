//! Four additive optical-depth/color bins, independent of draw order.
//! Four attachments fit the default 32-byte/color-sample limit with RGBA16F color.
pub(in crate::renderer) struct CloudTargets {
    _textures: [wgpu::Texture; 4],
    pub(in crate::renderer) views: [wgpu::TextureView; 4],
    pub(in crate::renderer) bind_group: wgpu::BindGroup,
}

pub(super) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let mut entries: Vec<_> = (0..4)
        .map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        })
        .collect();
    entries.push(super::pipeline::depth_entry(4));
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("cloud layer composite layout"),
        entries: &entries,
    })
}

pub(super) fn bytes(
    size: winit::dpi::PhysicalSize<u32>,
    scale: f32,
    format: wgpu::TextureFormat,
) -> u64 {
    u64::from((size.width as f32 * scale).ceil().max(1.0) as u32)
        * u64::from((size.height as f32 * scale).ceil().max(1.0) as u32)
        * if format == wgpu::TextureFormat::Rgba16Float {
            32
        } else {
            16
        }
}

impl CloudTargets {
    pub(super) fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        size: winit::dpi::PhysicalSize<u32>,
        depth: &wgpu::TextureView,
        scale: f32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let textures = std::array::from_fn(|_| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("scaled cloud optical bin"),
                size: wgpu::Extent3d {
                    width: (size.width as f32 * scale).ceil().max(1.0) as u32,
                    height: (size.height as f32 * scale).ceil().max(1.0) as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        });
        let views = textures
            .each_ref()
            .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()));
        let mut entries: Vec<_> = views
            .iter()
            .enumerate()
            .map(|(binding, view)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: wgpu::BindingResource::TextureView(view),
            })
            .collect();
        entries.push(wgpu::BindGroupEntry {
            binding: 4,
            resource: wgpu::BindingResource::TextureView(depth),
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cloud layer composite"),
            layout,
            entries: &entries,
        });
        Self {
            _textures: textures,
            views,
            bind_group,
        }
    }
}
