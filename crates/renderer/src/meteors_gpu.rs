//! Fixed-capacity cosmetic emission producer for the shared finite-distance layer.
use crate::meteors::{MeteorFrame, MAX_METEOR_CORES, MAX_TRAIL_POINTS};

const CAPACITY: usize = MAX_METEOR_CORES + MAX_TRAIL_POINTS;
const MIN_HEIGHT_M: f64 = 20_000.0;
const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];

#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct Segment {
    start: [f32; 4],
    end: [f32; 4],
    color: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<Segment>() == 48);
const _: () = assert!(std::mem::offset_of!(Segment, start) == 0);
const _: () = assert!(std::mem::offset_of!(Segment, end) == 16);
const _: () = assert!(std::mem::offset_of!(Segment, color) == 32);

pub struct MeteorGpu {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    pub count: u32,
    pub upload_bytes: u64,
    pub rejected_low: u32,
    pub draw_count: std::cell::Cell<u32>,
}

impl MeteorGpu {
    pub const BYTES: u64 = (CAPACITY * std::mem::size_of::<Segment>()) as u64;

    pub fn new(
        device: &wgpu::Device,
        camera: &wgpu::BindGroupLayout,
        depth: &wgpu::BindGroupLayout,
    ) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cosmetic meteor instances"),
            size: Self::BYTES,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cosmetic meteor emission"),
            source: wgpu::ShaderSource::Wgsl(crate::shader_source::meteors().into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cosmetic meteor emission"),
            bind_group_layouts: &[Some(camera), Some(depth)],
            immediate_size: 0,
        });
        let target = Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba16Float,
            blend: Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            }),
            write_mask: wgpu::ColorWrites::ALL,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cosmetic meteor emission"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 48,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &ATTRIBUTES,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[target.clone(), target],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            buffer,
            count: 0,
            upload_bytes: 0,
            rejected_low: 0,
            draw_count: std::cell::Cell::new(0),
        }
    }

    pub fn upload(&mut self, queue: &wgpu::Queue, frame: &MeteorFrame) {
        let mut segments = [Segment::default(); CAPACITY];
        self.count = 0;
        self.draw_count.set(0);
        self.rejected_low = 0;
        for core in frame.cores() {
            if frame.physical_height(core.position_rel_m) < MIN_HEIGHT_M {
                self.rejected_low += 1;
                continue;
            }
            let color = [
                core.linear_rgb[0],
                core.linear_rgb[1],
                core.linear_rgb[2],
                core.intensity / (std::f32::consts::PI * core.radius_m.powi(2)).max(1e-8),
            ];
            let first_segment = self.count as usize;
            let head = [
                core.position_rel_m[0],
                core.position_rel_m[1],
                core.position_rel_m[2],
                core.radius_m,
            ];
            segments[self.count as usize] = Segment {
                start: head,
                end: head,
                color,
            };
            self.count += 1;
            let mut previous: Option<&crate::meteors::TrailPoint> = None;
            for point in frame.trails().iter().filter(|point| point.id == core.id) {
                if frame.physical_height(point.position_rel_m) < MIN_HEIGHT_M {
                    previous = None;
                    continue;
                }
                if let Some(a) = previous {
                    let mut emission = color;
                    emission[3] *= a.opacity.min(point.opacity);
                    segments[self.count as usize] = Segment {
                        start: [
                            a.position_rel_m[0],
                            a.position_rel_m[1],
                            a.position_rel_m[2],
                            a.thickness_m,
                        ],
                        end: [
                            point.position_rel_m[0],
                            point.position_rel_m[1],
                            point.position_rel_m[2],
                            point.thickness_m,
                        ],
                        color: emission,
                    };
                    self.count += 1;
                }
                previous = Some(point);
            }
            if let Some(a) = previous {
                let mut emission = color;
                emission[3] *= a.opacity;
                segments[self.count as usize] = Segment {
                    start: [
                        a.position_rel_m[0],
                        a.position_rel_m[1],
                        a.position_rel_m[2],
                        a.thickness_m,
                    ],
                    end: [head[0], head[1], head[2], a.thickness_m],
                    color: emission,
                };
                self.count += 1;
            }
            let end = self.count as usize;
            let trail_area: f32 = segments[first_segment + 1..end]
                .iter()
                .map(|s| {
                    let a = glam::Vec3::from_array([s.start[0], s.start[1], s.start[2]]);
                    let b = glam::Vec3::from_array([s.end[0], s.end[1], s.end[2]]);
                    a.distance(b) * (s.start[3] + s.end[3]) * 0.5 * std::f32::consts::PI.sqrt()
                })
                .sum();
            if trail_area > 1e-6 {
                // Share the finite source power with its wake. A longer line
                // spreads that allocation instead of manufacturing more light.
                segments[first_segment].color[3] *= 0.85;
                let ratio = 0.15 * std::f32::consts::PI * core.radius_m.powi(2) / trail_area;
                for segment in &mut segments[first_segment + 1..end] {
                    segment.color[3] *= ratio;
                }
            }
        }
        let bytes = bytemuck::cast_slice(&segments[..self.count as usize]);
        self.upload_bytes = bytes.len() as u64;
        if !bytes.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytes);
        }
    }

    pub(crate) fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera: &'a wgpu::BindGroup,
        depth: &'a wgpu::BindGroup,
    ) -> u32 {
        if self.count == 0 {
            return 0;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, depth, &[]);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..6, 0..self.count);
        self.draw_count.set(1);
        1
    }
}
