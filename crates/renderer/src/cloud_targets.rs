pub const MAX_TARGET_BYTES: u64 = 64 * 1024 * 1024;

pub struct CloudTargets {
    pub accum: wgpu::TextureView,
    pub reveal: wgpu::TextureView,
    pub group: wgpu::BindGroup,
    pub bytes: u64,
    pub size: [u32; 2],
}

pub fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let entries: Vec<_> = (0..2)
        .map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        })
        .collect();
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("cloud OIT composition"),
        entries: &entries,
    })
}

impl CloudTargets {
    pub fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, size: [u32; 2]) -> Self {
        // Only target resolution degrades; cloud identity and voxel population do not.
        let factor = ((size[0] as f64 * size[1] as f64 * 10.0 / MAX_TARGET_BYTES as f64).sqrt())
            .max(size[0].max(size[1]) as f64 / device.limits().max_texture_dimension_2d as f64)
            .max(1.0);
        let width = (size[0] as f64 / factor).floor().max(1.0) as u32;
        let height = (size[1] as f64 / factor).floor().max(1.0) as u32;
        let texture = |label, format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
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
                .create_view(&Default::default())
        };
        let accum = texture(
            "cloud weighted accumulation",
            wgpu::TextureFormat::Rgba16Float,
        );
        let reveal = texture("cloud revealage", wgpu::TextureFormat::R16Float);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cloud OIT textures"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&accum),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&reveal),
                },
            ],
        });
        Self {
            accum,
            reveal,
            group,
            bytes: u64::from(width) * u64::from(height) * 10,
            size: [width, height],
        }
    }
}
