pub(super) fn depth_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

pub(super) fn shadow_entry() -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: 3,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}

pub(super) fn cloud_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    aggregates: bool,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("geometric clouds"),
        source: wgpu::ShaderSource::Wgsl(super::super::shader_source::clouds(aggregates).into()),
    });
    let buffers = [Some(super::super::cloud_mesh::CloudMeshes::vertex_layout())];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("geometric clouds"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_cloud"),
            compilation_options: Default::default(),
            buffers: if aggregates { &buffers } else { &[] },
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_cloud"),
            compilation_options: Default::default(),
            targets: &std::array::from_fn::<_, 4, _>(|_| {
                Some(wgpu::ColorTargetState {
                    format,
                    blend: Some({
                        let additive = wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        };
                        wgpu::BlendState {
                            color: additive,
                            alpha: additive,
                        }
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })
            }),
        }),
        primitive: wgpu::PrimitiveState {
            // The Earth camera mirrors X; mesh winding is outward in world space.
            front_face: wgpu::FrontFace::Cw,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub(super) fn depth_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    depth: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cloud scene depth"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(depth),
        }],
    })
}

pub(super) fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    label: &str,
    source: String,
    entries: (&str, &str),
    depth_compare: Option<wgpu::CompareFunction>,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some(entries.0),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(entries.1),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: depth_compare.map(|compare| wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(compare),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}
