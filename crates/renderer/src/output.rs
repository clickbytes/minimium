use std::borrow::Cow;
use wgpu::util::DeviceExt;

/// Time between two presented frames of the same world, view and clock revision.
pub struct ExposureInterval {
    pub real: std::time::Duration,
    pub world: std::time::Duration,
}

pub struct OutputEncoder {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    params: wgpu::Buffer,
    eye: wgpu::Buffer,
    meter: Option<crate::output_meter::FrameMeter>,
    metering_active: std::cell::Cell<bool>,
    pipeline: wgpu::RenderPipeline,
    hdr_enabled: bool,
    surface_is_srgb: bool,
}

impl OutputEncoder {
    pub fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        surface_format: wgpu::TextureFormat,
        scene_format: wgpu::TextureFormat,
        hdr_enabled: bool,
        world_frame_layout: &wgpu::BindGroupLayout,
        shadows: &wgpu::BindGroupLayout,
    ) -> Self {
        let texture = output_texture(device, size, scene_format);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("linear-output-params"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut eye_usage = wgpu::BufferUsages::UNIFORM;
        if hdr_enabled {
            eye_usage |= wgpu::BufferUsages::STORAGE;
        }
        let eye = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("absolute eye exposure and relative output gain"),
            contents: bytemuck::cast_slice(&[0.0_f32, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
            usage: eye_usage,
        });
        let bind_group_layout = output_layout(
            device,
            wgpu::ShaderStages::FRAGMENT,
            wgpu::BufferBindingType::Uniform,
        );
        let bind_group = output_bind_group(device, &bind_group_layout, &view, &params, &eye);
        let meter = hdr_enabled.then(|| {
            crate::output_meter::FrameMeter::new(
                device,
                world_frame_layout,
                shadows,
                &view,
                &params,
                &eye,
            )
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("linear-output-srgb-encode-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(crate::shader_source::encode_output())),
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
            eye,
            meter,
            metering_active: std::cell::Cell::new(false),
            pipeline,
            hdr_enabled,
            surface_is_srgb: is_srgb(surface_format),
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        self.texture = output_texture(device, size, self.texture.format());
        self.view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group = output_bind_group(
            device,
            &self.bind_group_layout,
            &self.view,
            &self.params,
            &self.eye,
        );
        if let Some(meter) = &mut self.meter {
            meter.resize(device, &self.view, &self.params, &self.eye);
        }
    }

    pub fn target_view(&self) -> &wgpu::TextureView {
        &self.view
    }
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn size_bytes(&self) -> u64 {
        let bytes_per_pixel = if self.texture.format() == wgpu::TextureFormat::Rgba16Float {
            8
        } else {
            4
        };
        self.texture.size().width as u64 * self.texture.size().height as u64 * bytes_per_pixel
            + self.params.size()
            + self.eye.size()
    }

    pub fn update_exposure(&self, queue: &wgpu::Queue, exposure: f32) {
        self.update_exposure_with_water(queue, exposure, 0.0);
    }

    pub fn update_exposure_with_water(
        &self,
        queue: &wgpu::Queue,
        exposure: f32,
        submerged_fraction: f32,
    ) {
        self.metering_active.set(false);
        self.write_params(
            queue,
            exposure,
            submerged_fraction,
            [0.0, 0.0, 0.0, -2.0, 0.0, 0.0],
        );
    }

    /// Request luminance measurement for this scene. A new world/view starts
    /// directly at its own measured exposure.
    pub fn meter_exposure_with_water(
        &self,
        queue: &wgpu::Queue,
        storage_gain: f32,
        base: f32,
        reference: f32,
        cadence: Option<ExposureInterval>,
        submerged_fraction: f32,
        fixed_gain: Option<f32>,
    ) {
        let base = base.max(0.00001);
        self.metering_active.set(self.hdr_enabled);
        let (elapsed, fast_forward) = cadence.map_or((-1.0, 0.0), |interval| {
            let elapsed = interval.real.as_secs_f32();
            let world = interval.world.as_secs_f32();
            // Render UTC has millisecond resolution. Ignore normal tick rounding.
            let extra = (world - elapsed - 0.001).max(0.0);
            (elapsed, extra / world.max(0.001))
        });
        self.write_params(
            queue,
            1.0,
            submerged_fraction,
            [
                storage_gain.max(0.00001).log2(),
                (base / 64.0).log2(),
                reference.max(base * 2.0).log2(),
                elapsed,
                fast_forward,
                fixed_gain.unwrap_or(0.0),
            ],
        );
    }

    pub(crate) fn meter_scene(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &wgpu::BindGroup,
        shadows: &wgpu::BindGroup,
    ) {
        if self.metering_active.get() {
            if let Some(meter) = &self.meter {
                meter.encode(encoder, frame, shadows);
            }
        }
    }

    fn write_params(
        &self,
        queue: &wgpu::Queue,
        exposure: f32,
        submerged_fraction: f32,
        meter: [f32; 6],
    ) {
        let params = [
            exposure,
            f32::from(self.hdr_enabled),
            f32::from(self.surface_is_srgb),
            submerged_fraction.clamp(0.0, 1.0),
            meter[0],
            meter[1],
            meter[2],
            meter[3],
            meter[4],
            meter[5],
            0.0,
            0.0,
        ];
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&params));
    }
}

fn output_texture(
    device: &wgpu::Device,
    size: [u32; 2],
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("linear-output-scene-target"),
        size: wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub(super) fn output_layout(
    device: &wgpu::Device,
    stage: wgpu::ShaderStages,
    eye_binding: wgpu::BufferBindingType,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("linear-output-texture-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: stage,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: stage,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(48),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: stage,
                ty: wgpu::BindingType::Buffer {
                    ty: eye_binding,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(32),
                },
                count: None,
            },
        ],
    })
}

pub(super) fn output_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    params: &wgpu::Buffer,
    eye: &wgpu::Buffer,
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
            wgpu::BindGroupEntry {
                binding: 2,
                resource: eye.as_entire_binding(),
            },
        ],
    })
}

fn is_srgb(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb
    )
}
