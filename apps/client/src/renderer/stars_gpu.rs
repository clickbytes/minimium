use super::{
    environment::stars::{StarInstanceGpu, STAR_GPU_RECORD_BYTES, STAR_GPU_RECORD_COUNT},
    environment::{EnvironmentFrame, StarFrameStamp},
};
use std::borrow::Cow;
use wgpu::util::DeviceExt;

pub(super) struct StarGpu {
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) buffer: wgpu::Buffer,
    uploaded_stamp: Option<StarFrameStamp>,
}

const ATTRIBUTES: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32x3, 3 => Uint32];

impl StarGpu {
    pub(super) fn new(
        device: &wgpu::Device,
        frame_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hyg-1000-star-instance-buffer"),
            contents: bytemuck::cast_slice(&vec![
                StarInstanceGpu::default();
                STAR_GPU_RECORD_COUNT
            ]),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("star-frame-pipeline-layout"),
            bind_group_layouts: &[Some(frame_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hyg-star-batch-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(super::shader_source::stars())),
        });
        let attributes = wgpu::VertexBufferLayout {
            array_stride: STAR_GPU_RECORD_BYTES as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ATTRIBUTES,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hyg-star-batch-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(attributes)],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            buffer,
            uploaded_stamp: None,
        }
    }

    pub(super) fn uploaded_stamp(&self) -> Option<StarFrameStamp> {
        self.uploaded_stamp
    }

    pub(super) fn upload(
        &mut self,
        queue: &wgpu::Queue,
        instances: &[StarInstanceGpu; STAR_GPU_RECORD_COUNT],
        stamp: StarFrameStamp,
    ) {
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(instances));
        self.uploaded_stamp = Some(stamp);
    }

    pub(super) fn matches_frame(&self, frame: &EnvironmentFrame) -> bool {
        frame.diagnostics.star_frame == self.uploaded_stamp
    }
}
