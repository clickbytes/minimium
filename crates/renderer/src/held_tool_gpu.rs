//! GPU pipeline for a camera-local handheld model overlay.
use crate::held_tool::{HeldToolModel, HeldToolModelError, HeldToolPose, HeldToolVertex};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct HeldToolUniform {
    transform: [[f32; 4]; 4],
    projection: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<HeldToolUniform>() == 80);

pub struct HeldToolGpu {
    pipeline: wgpu::RenderPipeline,
    models: Vec<(wgpu::Buffer, u32)>,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl HeldToolGpu {
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        models: &[HeldToolModel],
    ) -> Result<Self, HeldToolModelError> {
        if models.is_empty() {
            return Err(HeldToolModelError::InvalidCatalog);
        }
        if models.len() > crate::held_tool::MAX_HELD_TOOL_MODELS {
            return Err(HeldToolModelError::Capacity);
        }
        let total_vertices = models
            .iter()
            .map(|model| model.vertices().len())
            .sum::<usize>();
        if total_vertices > crate::held_tool::MAX_HELD_TOOL_TOTAL_VERTICES {
            return Err(HeldToolModelError::Capacity);
        }
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("held-tool-pose"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bindings = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("held-tool-pose-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(80),
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("held-tool-pose-bind-group"),
            layout: &bindings,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("held-tool-pipeline-layout"),
            bind_group_layouts: &[Some(&bindings)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("held-tool-viewmodel-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/held_tool.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("held-tool-viewmodel-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(vertex_layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(if color_format == wgpu::TextureFormat::Rgba16Float {
                    "fs_hdr_overlay"
                } else {
                    "fs_main"
                }),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
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
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let models = models
            .iter()
            .enumerate()
            .map(|(index, model)| mesh_buffer(device, &format!("held-tool-{index}"), model))
            .collect();
        Ok(Self {
            pipeline,
            models,
            uniform,
            bind_group,
        })
    }

    pub fn upload(&self, queue: &wgpu::Queue, pose: HeldToolPose, viewport: [u32; 2]) {
        let aspect = viewport[0].max(1) as f32 / viewport[1].max(1) as f32;
        let focal = 1.55;
        let near = 0.05;
        let far = 4.0;
        let uniform = HeldToolUniform {
            transform: pose.transform(),
            projection: [
                focal / aspect,
                focal,
                -far / (far - near),
                -far * near / (far - near),
            ],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        model_index: usize,
    ) -> u32 {
        let Some((buffer, count)) = self.models.get(model_index) else {
            return 0;
        };
        if *count == 0 {
            return 0;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("held-tool-viewmodel-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            // World depth is no longer consumed after cloud/dust composition.
            // Reuse it for self-occlusion, independent of world occlusion.
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..*count, 0..1);
        1
    }

    pub fn size_bytes(&self) -> u64 {
        self.uniform.size()
            + self
                .models
                .iter()
                .map(|(buffer, _)| buffer.size())
                .sum::<u64>()
    }
}

fn mesh_buffer(device: &wgpu::Device, label: &str, model: &HeldToolModel) -> (wgpu::Buffer, u32) {
    let vertices = model.vertices();
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    (buffer, vertices.len() as u32)
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<HeldToolVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}
