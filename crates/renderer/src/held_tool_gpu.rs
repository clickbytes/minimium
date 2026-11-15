//! GPU pipeline for a camera-local handheld model overlay.
use crate::held_tool::{self, HeldToolVertex};
use minimium_world::items::ToolKind;
use wgpu::util::DeviceExt;

pub struct HeldToolGpu {
    pipeline: wgpu::RenderPipeline,
    shovel: wgpu::Buffer,
    shovel_count: u32,
    pickaxe: wgpu::Buffer,
    pickaxe_count: u32,
}

impl HeldToolGpu {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("held-tool-pipeline-layout"),
            bind_group_layouts: &[],
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
                entry_point: Some("fs_main"),
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
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let (shovel, shovel_count) = mesh_buffer(device, "held-shovel", ToolKind::Shovel);
        let (pickaxe, pickaxe_count) = mesh_buffer(device, "held-pickaxe", ToolKind::Pickaxe);
        Self {
            pipeline,
            shovel,
            shovel_count,
            pickaxe,
            pickaxe_count,
        }
    }

    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        kind: ToolKind,
    ) -> u32 {
        let (buffer, count) = match kind {
            ToolKind::Shovel => (&self.shovel, self.shovel_count),
            ToolKind::Pickaxe => (&self.pickaxe, self.pickaxe_count),
        };
        if count == 0 {
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
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..count, 0..1);
        1
    }

    pub fn size_bytes(&self) -> u64 {
        self.shovel.size() + self.pickaxe.size()
    }
}

fn mesh_buffer(device: &wgpu::Device, label: &str, kind: ToolKind) -> (wgpu::Buffer, u32) {
    let vertices = held_tool::vertices(kind);
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(&vertices),
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
