use std::borrow::Cow;
use winit::dpi::PhysicalSize;

pub(super) struct OutputEncoder {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    params: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    hdr_enabled: bool,
    surface_is_srgb: bool,
}

impl OutputEncoder {
    pub(super) fn new(
        device: &wgpu::Device,
        size: PhysicalSize<u32>,
        surface_format: wgpu::TextureFormat,
        scene_format: wgpu::TextureFormat,
        hdr_enabled: bool,
    ) -> Self {
        let texture = output_texture(device, size, scene_format);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("linear-output-params"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("linear-output-texture-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
            ],
        });
        let bind_group = output_bind_group(device, &bind_group_layout, &view, &params);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("linear-output-srgb-encode-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(super::shader_source::encode_output())),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("linear-output-srgb-encode-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("linear-output-srgb-encode-pipeline"),
            layout: Some(&pipeline_layout),
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
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            texture,
            view,
            bind_group_layout,
            bind_group,
            params,
            pipeline,
            hdr_enabled,
            surface_is_srgb: super::gpu_surface::is_srgb(surface_format),
        }
    }

    pub(super) fn resize(&mut self, device: &wgpu::Device, size: PhysicalSize<u32>) {
        self.texture = output_texture(device, size, self.texture.format());
        self.view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group =
            output_bind_group(device, &self.bind_group_layout, &self.view, &self.params);
    }

    pub(super) fn target_view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub(super) fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    pub(super) fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub(super) fn size_bytes(&self) -> u64 {
        let bytes_per_pixel = if self.texture.format() == wgpu::TextureFormat::Rgba16Float {
            8
        } else {
            4
        };
        self.texture.size().width as u64 * self.texture.size().height as u64 * bytes_per_pixel
            + self.params.size()
    }

    pub(super) fn update_exposure(&self, queue: &wgpu::Queue, exposure: f32) {
        let params = [
            exposure,
            f32::from(self.hdr_enabled),
            f32::from(self.surface_is_srgb),
            0.0,
        ];
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&params));
    }
}

fn output_texture(
    device: &wgpu::Device,
    size: PhysicalSize<u32>,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("linear-output-scene-target"),
        size: wgpu::Extent3d {
            width: size.width.max(1),
            height: size.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn output_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    params: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("linear-output-texture-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: params.as_entire_binding(),
            },
        ],
    })
}
