use crate::{
    cloud_face::{CloudFace as Face, MAX_CLOUD_FACES},
    cloud_pipeline,
    cloud_targets::CloudTargets,
    cloud_uniform::{CloudRecord, CloudUniform},
};
use minimium_environment::weather::MAX_CLOUDS;

const _: () = assert!(std::mem::size_of::<Face>() == 48);

pub struct CloudGpu {
    pub pipeline: wgpu::RenderPipeline,
    pub debug_pipeline: wgpu::RenderPipeline,
    pub(crate) bolid_mask_pipeline: wgpu::RenderPipeline,
    pub composite_pipeline: Option<wgpu::RenderPipeline>,
    pub group: wgpu::BindGroup,
    pub targets: Option<CloudTargets>,
    pub uniform: wgpu::Buffer,
    pub records: wgpu::Buffer,
    pub faces: wgpu::Buffer,
    pub count: u32,
    pub revision: u64,
    pub debug: bool,
    pub oit: bool,
    layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
}

impl CloudGpu {
    pub const BUFFER_BYTES: u64 = (MAX_CLOUD_FACES * 48 + MAX_CLOUDS * 80 + 256) as u64;

    pub fn new(
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        frame: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        depth: &wgpu::TextureView,
        size: [u32; 2],
    ) -> Self {
        let oit = [
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureFormat::R16Float,
        ]
        .iter()
        .all(|format| {
            let f = adapter.get_texture_format_features(*format);
            f.allowed_usages.contains(
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            ) && f.flags.contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
        }) && device.limits().max_color_attachment_bytes_per_sample >= 10;
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let uniform = buffer("cloud uniform", 256, wgpu::BufferUsages::UNIFORM);
        let records = buffer(
            "cloud transforms",
            (MAX_CLOUDS * 80) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let faces = buffer(
            "bounded greedy cloud faces",
            (MAX_CLOUD_FACES * 48) as u64,
            wgpu::BufferUsages::VERTEX,
        );
        let layout = cloud_pipeline::layout(device);
        let composite_layout = crate::cloud_targets::layout(device);
        let group = cloud_pipeline::group(device, &layout, &uniform, &records, depth);
        let pipeline = cloud_pipeline::create(device, frame, &layout, format, oit, false, false);
        let debug_pipeline =
            cloud_pipeline::create(device, frame, &layout, format, false, true, false);
        let composite_pipeline =
            oit.then(|| cloud_pipeline::composite(device, &composite_layout, format));
        let targets = oit.then(|| CloudTargets::new(device, &composite_layout, [size[0], size[1]]));
        Self {
            pipeline,
            debug_pipeline,
            bolid_mask_pipeline: cloud_pipeline::create(
                device, frame, &layout, format, false, false, true,
            ),
            composite_pipeline,
            group,
            targets,
            uniform,
            records,
            faces,
            count: 0,
            revision: 0,
            debug: false,
            oit,
            layout,
            composite_layout,
        }
    }

    pub fn update_faces(&mut self, queue: &wgpu::Queue, faces: &[Face], revision: u64) {
        if self.revision == revision {
            return;
        }
        if !faces.is_empty() {
            queue.write_buffer(&self.faces, 0, bytemuck::cast_slice(faces));
        }
        self.count = faces.len() as u32;
        self.revision = revision;
    }

    pub fn upload_frame(
        &self,
        queue: &wgpu::Queue,
        records: &[CloudRecord],
        uniform: &CloudUniform,
    ) {
        if !records.is_empty() {
            queue.write_buffer(&self.records, 0, bytemuck::cast_slice(records));
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(uniform));
    }

    /// Encodes the cloud face, transparency composite and optional debug passes.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        camera: &wgpu::BindGroup,
        scene_visible: bool,
    ) -> u32 {
        if !scene_visible || (self.count == 0 && !self.debug) {
            return 0;
        }
        let mut draws = 0;
        if self.count > 0 {
            let attachments = if let Some(targets) = &self.targets {
                vec![
                    color_attachment(
                        &targets.accum,
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    ),
                    color_attachment(&targets.reveal, wgpu::LoadOp::Clear(wgpu::Color::WHITE)),
                ]
            } else {
                vec![color_attachment(scene, wgpu::LoadOp::Load)]
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("regional cube clouds"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, camera, &[]);
            pass.set_bind_group(1, &self.group, &[]);
            pass.set_vertex_buffer(0, self.faces.slice(..));
            pass.draw(0..6, 0..self.count);
            draws += 1;
        }
        if self.count > 0 {
            if let (Some(pipeline), Some(targets)) = (&self.composite_pipeline, &self.targets) {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("cloud transparency composite"),
                    color_attachments: &[color_attachment(scene, wgpu::LoadOp::Load)],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &targets.group, &[]);
                pass.draw(0..3, 0..1);
                draws += 1;
            }
        }
        if self.debug {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("weather regions and wind"),
                color_attachments: &[color_attachment(scene, wgpu::LoadOp::Load)],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.debug_pipeline);
            pass.set_bind_group(0, camera, &[]);
            pass.set_bind_group(1, &self.group, &[]);
            pass.draw(0..3, 0..1);
            draws += 1;
        }
        draws
    }

    pub fn resize(&mut self, device: &wgpu::Device, depth: &wgpu::TextureView, size: [u32; 2]) {
        self.group =
            cloud_pipeline::group(device, &self.layout, &self.uniform, &self.records, depth);
        self.targets = self
            .oit
            .then(|| CloudTargets::new(device, &self.composite_layout, [size[0], size[1]]));
    }

    pub fn bytes(&self) -> u64 {
        Self::BUFFER_BYTES + self.targets.as_ref().map_or(0, |t| t.bytes)
    }
}

fn color_attachment(
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> Option<wgpu::RenderPassColorAttachment<'_>> {
    Some(wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load,
            store: wgpu::StoreOp::Store,
        },
    })
}
