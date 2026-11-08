//! Rare floating-origin shifts update resident vertices without rebuilding meshes.
use wgpu::util::DeviceExt;
const STRIDES: [u32; 3] = [9, 28, 6];
pub struct GeometryRebase {
    pipeline: wgpu::ComputePipeline,
    parameters: [wgpu::Buffer; STRIDES.len()],
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
                contents: &[0; 16],
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
        buffers: impl Iterator<Item = (&'a wgpu::Buffer, u32)>,
    ) {
        for (i, stride) in STRIDES.into_iter().enumerate() {
            let words = [
                delta[0].to_bits(),
                delta[1].to_bits(),
                delta[2].to_bits(),
                stride,
            ];
            queue.write_buffer(&self.parameters[i], 0, bytemuck::cast_slice(&words));
        }
        let layout = self.pipeline.get_bind_group_layout(0);
        let groups: Vec<_> = buffers
            .filter(|(b, s)| b.size() >= u64::from(*s) * 4)
            .map(|(buffer, stride)| {
                let index = STRIDES
                    .iter()
                    .position(|s| *s == stride)
                    .expect("supported render vertex layout");
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
                            resource: self.parameters[index].as_entire_binding(),
                        },
                    ],
                });
                let vertices = (buffer.size() / u64::from(stride) / 4) as u32;
                // Terrain updates one complete triangle and its plane normal.
                let invocations = if stride == 28 { vertices / 3 } else { vertices };
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
