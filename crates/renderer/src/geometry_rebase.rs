//! Rare floating-origin shifts update resident vertices without rebuilding meshes.
use crate::{geometry::Vertex, ocean_geometry::OceanVertex, terrain_geometry::TerrainVertex};
use std::mem::{offset_of, size_of};
use wgpu::util::DeviceExt;

#[derive(Clone, Copy)]
pub(crate) enum RebaseLayout {
    Object,
    Terrain,
    Water,
}

impl RebaseLayout {
    const ALL: [Self; 3] = [Self::Object, Self::Terrain, Self::Water];

    fn stride(self) -> u32 {
        let bytes = match self {
            Self::Object => size_of::<Vertex>(),
            Self::Terrain => size_of::<TerrainVertex>(),
            Self::Water => size_of::<OceanVertex>(),
        };
        (bytes / size_of::<f32>()) as u32
    }

    fn vertices_per_invocation(self) -> u32 {
        match self {
            Self::Terrain => 3,
            Self::Object | Self::Water => 1,
        }
    }

    fn normal_offset(self) -> u32 {
        match self {
            Self::Terrain => {
                (offset_of!(TerrainVertex, geometric_normal) / size_of::<f32>()) as u32
            }
            Self::Object | Self::Water => 0,
        }
    }
}

const _: () = {
    assert!(offset_of!(Vertex, position) == 0);
    assert!(offset_of!(TerrainVertex, position) == 0);
    assert!(offset_of!(OceanVertex, position) == 0);
};

pub struct GeometryRebase {
    pipeline: wgpu::ComputePipeline,
    parameters: [wgpu::Buffer; RebaseLayout::ALL.len()],
}
impl GeometryRebase {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("geometry rebase"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/geometry_rebase.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("geometry rebase"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let parameters = std::array::from_fn(|_| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("geometry rebase delta"),
                contents: &[0; 32],
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })
        });
        Self {
            pipeline,
            parameters,
        }
    }
    pub fn apply<'a>(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        delta: [f32; 3],
        buffers: impl Iterator<Item = (&'a wgpu::Buffer, RebaseLayout)>,
    ) {
        for layout in RebaseLayout::ALL {
            let words = [
                delta[0].to_bits(),
                delta[1].to_bits(),
                delta[2].to_bits(),
                layout.stride(),
                layout.vertices_per_invocation(),
                layout.normal_offset(),
                0,
                0,
            ];
            queue.write_buffer(
                &self.parameters[layout as usize],
                0,
                bytemuck::cast_slice(&words),
            );
        }
        let layout = self.pipeline.get_bind_group_layout(0);
        let groups: Vec<_> = buffers
            .filter(|(b, layout)| b.size() >= u64::from(layout.stride()) * 4)
            .map(|(buffer, vertex_layout)| {
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("geometry rebase buffer"),
                    layout: &layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: self.parameters[vertex_layout as usize].as_entire_binding(),
                        },
                    ],
                });
                let vertices = (buffer.size() / u64::from(vertex_layout.stride()) / 4) as u32;
                // Terrain updates one complete triangle and its plane normal.
                let invocations = vertices / vertex_layout.vertices_per_invocation();
                (group, invocations)
            })
            .collect();
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            for (group, count) in &groups {
                pass.set_bind_group(0, group, &[]);
                pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
            }
        }
        queue.submit([encoder.finish()]);
    }
}
