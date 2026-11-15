//! Bounded emissive layer with radiance-weighted distance and cloud transmission.
pub const BYTES_PER_PIXEL: u64 = 34;

pub struct BolidLayer {
    pub diagnostic: wgpu::Buffer,
    pub color: wgpu::TextureView,
    pub dust: wgpu::TextureView,
    pub distance: wgpu::TextureView,
    pub transmission: wgpu::TextureView,
    pub foreground: wgpu::TextureView,
    pub distance_group: wgpu::BindGroup,
    pub composite_group: wgpu::BindGroup,
    pub size: [u32; 2],
}

fn composite_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let mut entries: Vec<_> = (0..4)
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
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: 4,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: std::num::NonZeroU64::new(16),
        },
        count: None,
    });
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bolid composite diagnostics"),
        entries: &entries,
    })
}

pub fn texture_layout(device: &wgpu::Device, count: u32) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bolid layer textures"),
        entries: &(0..count)
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
            .collect::<Vec<_>>(),
    })
}

impl BolidLayer {
    pub fn new(device: &wgpu::Device, size: [u32; 2]) -> Self {
        // 34 bytes/pixel, capped at 64 MiB independently of window resolution.
        let factor = ((size[0] as f64 * size[1] as f64 * BYTES_PER_PIXEL as f64
            / (64.0 * 1024.0 * 1024.0))
            .sqrt())
        .max(1.0);
        let size = size.map(|v| (v as f64 / factor).floor().max(1.0) as u32);
        let texture = |label, format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
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
        let color = texture("bolid linear emission", wgpu::TextureFormat::Rgba16Float);
        let dust = texture("bolid dust scattering", wgpu::TextureFormat::Rgba16Float);
        let distance = texture("bolid weighted distance", wgpu::TextureFormat::Rgba16Float);
        let foreground = texture(
            "foreground cloud radiance",
            wgpu::TextureFormat::Rgba16Float,
        );
        let transmission = texture(
            "cloud transmission to bolid",
            // Multiplicative extinction must retain values well below 1/255.
            // UNORM rounding can stall attenuation and leak a bright core.
            wgpu::TextureFormat::R16Float,
        );
        let group = |layout: &wgpu::BindGroupLayout, views: &[&wgpu::TextureView]| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bolid layer"),
                layout,
                entries: &views
                    .iter()
                    .enumerate()
                    .map(|(i, view)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: wgpu::BindingResource::TextureView(view),
                    })
                    .collect::<Vec<_>>(),
            })
        };
        let distance_group = group(&texture_layout(device, 1), &[&distance]);
        let diagnostic = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bolid transmission diagnostic"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut entries: Vec<_> = [&color, &transmission, &foreground, &dust]
            .into_iter()
            .enumerate()
            .map(|(binding, view)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: wgpu::BindingResource::TextureView(view),
            })
            .collect();
        entries.push(wgpu::BindGroupEntry {
            binding: 4,
            resource: diagnostic.as_entire_binding(),
        });
        let composite_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bolid composite"),
            layout: &composite_layout(device),
            entries: &entries,
        });
        Self {
            diagnostic,
            color,
            dust,
            distance,
            transmission,
            foreground,
            distance_group,
            composite_group,
            size,
        }
    }
}

pub fn composite_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let textures = composite_layout(device);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("bolid layer resolve"),
        bind_group_layouts: &[Some(&textures)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bolid layer resolve"),
        source: wgpu::ShaderSource::Wgsl(crate::shader_source::bolid_composite().into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bolid layer resolve"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(if format == wgpu::TextureFormat::Rgba16Float {
                "fs_main"
            } else {
                "fs_fallback"
            }),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}
