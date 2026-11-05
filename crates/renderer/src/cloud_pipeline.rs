use crate::CloudFace;

pub fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let buffer = |binding, ty, size| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(size),
        },
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("regional cloud resources"),
        entries: &[
            buffer(0, wgpu::BufferBindingType::Uniform, 256),
            buffer(1, wgpu::BufferBindingType::Storage { read_only: true }, 80),
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

pub fn group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    records: &wgpu::Buffer,
    depth: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("regional cloud frame"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: records.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(depth),
            },
        ],
    })
}

pub fn create(
    device: &wgpu::Device,
    frame: &wgpu::BindGroupLayout,
    cloud: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    oit: bool,
    debug: bool,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("cloud pipeline layout"),
        bind_group_layouts: &[Some(frame), Some(cloud)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cube cloud shader"),
        source: wgpu::ShaderSource::Wgsl(crate::shader_source::clouds().into()),
    });
    let additive = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    };
    let reveal = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::OneMinusSrc,
        operation: wgpu::BlendOperation::Add,
    };
    let target = |format, blend| {
        Some(wgpu::ColorTargetState {
            format,
            blend: Some(blend),
            write_mask: wgpu::ColorWrites::ALL,
        })
    };
    let targets = if oit {
        vec![
            target(
                wgpu::TextureFormat::Rgba16Float,
                wgpu::BlendState {
                    color: additive,
                    alpha: additive,
                },
            ),
            target(
                wgpu::TextureFormat::R16Float,
                wgpu::BlendState {
                    color: reveal,
                    alpha: reveal,
                },
            ),
        ]
    } else {
        vec![target(format, wgpu::BlendState::ALPHA_BLENDING)]
    };
    let buffers = [Some(wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<CloudFace>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4],
    })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("cube clouds"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some(if debug { "vs_debug" } else { "vs_cloud" }),
            compilation_options: Default::default(),
            buffers: if debug { &[] } else { &buffers },
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(if debug {
                "fs_debug"
            } else if oit {
                "fs_oit"
            } else {
                "fs_alpha"
            }),
            compilation_options: Default::default(),
            targets: &targets,
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub fn composite(
    device: &wgpu::Device,
    textures: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("cloud composite layout"),
        bind_group_layouts: &[Some(textures)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cloud composite shader"),
        source: wgpu::ShaderSource::Wgsl(crate::shader_source::cloud_composite().into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("cloud OIT resolve"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
