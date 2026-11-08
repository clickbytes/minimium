//! Shared atmosphere LUT owner and linear HDR composition.
use crate::{
    atmosphere_pipeline as pipeline, atmosphere_resources::AtmosphereResources, RenderViewFrame,
};
pub struct AtmosphereGpu {
    pub pipeline: wgpu::RenderPipeline,
    pub depth_bind_group: wgpu::BindGroup,
    pub resources: AtmosphereResources,
    depth_layout: wgpu::BindGroupLayout,
    scratch: wgpu::Texture,
    compute: Vec<wgpu::ComputePipeline>,
    outputs: Vec<wgpu::BindGroup>,
    active: bool,
    standard: bool,
    last_radius: f64,
    dirty: std::cell::Cell<bool>,
    weather: Option<crate::atmosphere_view::WeatherEndpoints>,
    pub debug: u32,
    timing: Option<crate::atmosphere_timing::AtmosphereTiming>,
    last_view: crate::AtmosphereUniform,
    needs_lut: std::cell::Cell<bool>,
    computed: std::cell::Cell<bool>,
    light_key: std::cell::Cell<Option<[[f32; 4]; 5]>>,
}
impl AtmosphereGpu {
    pub const BYTES: u64 = AtmosphereResources::BYTES;
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        depth: &wgpu::TextureView,
        resources: AtmosphereResources,
    ) -> Self {
        let depth_layout = pipeline::scene_layout(device);
        let scratch = Self::scratch(device, depth.texture().size(), format);
        let depth_bind_group = pipeline::depth_group(
            device,
            &depth_layout,
            depth,
            &scratch.create_view(&Default::default()),
        );
        let render = pipeline::create(device, frame, &depth_layout, format);
        let (compute, outputs) = if resources.physical {
            let out2 = pipeline::outputs(device, wgpu::TextureViewDimension::D2);
            let out3 = pipeline::outputs(device, wgpu::TextureViewDimension::D3);
            let output = |a, b, layout: &wgpu::BindGroupLayout| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("atmosphere output textures"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&resources.stages[a]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&resources.stages[b]),
                        },
                    ],
                })
            };
            let outputs = vec![
                output(4, 1, &out2),
                output(0, 1, &out2),
                output(2, 3, &out3),
            ];
            let compute = vec![
                pipeline::compute(device, frame, &out2, "multi"),
                pipeline::compute(device, frame, &out2, "sky"),
                pipeline::compute(device, frame, &out3, "aerial"),
            ];
            (compute, outputs)
        } else {
            (Vec::new(), Vec::new())
        };
        Self {
            pipeline: render,
            depth_bind_group,
            resources,
            depth_layout,
            scratch,
            compute,
            outputs,
            active: false,
            standard: true,
            last_radius: 0.0,
            dirty: std::cell::Cell::new(true),
            weather: None,
            debug: 0,
            timing: crate::atmosphere_timing::AtmosphereTiming::new(device, queue),
            last_view: Default::default(),
            needs_lut: std::cell::Cell::new(true),
            computed: std::cell::Cell::new(false),
            light_key: std::cell::Cell::new(None),
        }
    }
    fn scratch(
        device: &wgpu::Device,
        size: wgpu::Extent3d,
        format: wgpu::TextureFormat,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmosphere scene snapshot"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }
    pub fn bytes(&self) -> u64 {
        self.resources.bytes()
            + if self.timing.is_some() { 512 } else { 0 }
            + self.scratch.size().width as u64
                * self.scratch.size().height as u64
                * if self.scratch.format() == wgpu::TextureFormat::Rgba16Float {
                    8
                } else {
                    4
                }
    }
    pub fn optical_uniform(&self) -> &crate::AtmosphereUniform {
        &self.last_view
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
    pub fn high_stars(&self) -> bool {
        self.last_view.config[0] > 0.0 && self.last_view.up_height[3] > 12000.0
    }
    pub fn set_weather(&mut self, queue: &wgpu::Queue, grid: &crate::surface_cover::CoverGrid) {
        self.weather = Some(crate::atmosphere_view::WeatherEndpoints::from_grid(grid));
        self.needs_lut.set(true);
        queue.write_texture(
            self.resources.fog.as_image_copy(),
            bytemuck::cast_slice(&grid.fog_pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(9 * 8),
                rows_per_image: Some(9),
            },
            self.resources.fog.size(),
        );
    }
    pub fn clear_weather(&mut self) {
        self.weather = None;
    }
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: RenderViewFrame,
        radius: f64,
        scale: f64,
        standard: bool,
        utc: f64,
    ) {
        if let Some(timing) = &self.timing {
            timing.poll(device);
        }
        self.standard = standard;
        self.active = radius > 0.0 && self.resources.physical;
        let value = crate::atmosphere_view::prepare(
            view,
            radius,
            scale,
            standard,
            utc,
            self.weather.as_ref(),
            self.debug,
            self.active,
        );
        // Fallback dust also samples this CPU-built optical-depth table.
        // Its radius must follow the view even when compute/HDR is disabled.
        if radius > 0.0 && (self.last_radius - value.radii[0] as f64).abs() > 0.1 {
            self.last_radius = value.radii[0] as f64;
            let mut profile = crate::atmosphere::EARTH;
            profile.radius_m = self.last_radius.max(1.0);
            crate::atmosphere_table::update(queue, self.resources.optical.texture(), profile);
            self.dirty.set(true);
        }
        queue.write_buffer(&self.resources.uniform, 0, bytemuck::bytes_of(&value));
        if bytemuck::bytes_of(&value) != bytemuck::bytes_of(&self.last_view) {
            self.needs_lut.set(true);
        }
        self.last_view = value;
    }
    pub fn resize(&mut self, device: &wgpu::Device, depth: &wgpu::TextureView) {
        self.scratch = Self::scratch(device, depth.texture().size(), self.scratch.format());
        self.depth_bind_group = pipeline::depth_group(
            device,
            &self.depth_layout,
            depth,
            &self.scratch.create_view(&Default::default()),
        );
    }
    pub fn prepare(&self, encoder: &mut wgpu::CommandEncoder, frame: &wgpu::BindGroup) {
        self.computed.set(false);
        if !self.active {
            return;
        }
        if !self.needs_lut.replace(false) {
            return;
        }
        self.computed.set(true);
        // Separate sampled and storage textures avoid read/write aliasing in bind groups.
        let dirty = self.dirty.replace(false);
        for i in 0..3 {
            if i == 0 && !dirty {
                continue;
            }
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("atmosphere LUT update"),
                    timestamp_writes: if i > 0 {
                        self.timing.as_ref().map(|t| t.compute((i - 1) as u32))
                    } else {
                        None
                    },
                });
                pass.set_pipeline(&self.compute[i]);
                pass.set_bind_group(0, frame, &[]);
                pass.set_bind_group(1, &self.outputs[i], &[]);
                match i {
                    0 => pass.dispatch_workgroups(4, 4, 1),
                    1 => {
                        if self.standard {
                            pass.dispatch_workgroups(24, 14, 1)
                        } else {
                            pass.dispatch_workgroups(16, 8, 1)
                        }
                    }
                    _ => {
                        if self.standard {
                            pass.dispatch_workgroups(8, 8, 8)
                        } else {
                            pass.dispatch_workgroups(4, 4, 8)
                        }
                    }
                }
            }
            match i {
                0 => self.resources.publish(encoder, 4..5),
                1 => self.resources.publish(encoder, 0..2),
                _ => self.resources.publish(encoder, 2..4),
            }
        }
    }
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        camera: &wgpu::BindGroup,
        source: Option<&wgpu::Texture>,
        enabled: bool,
    ) -> u32 {
        if !enabled || !self.active {
            return 0;
        }
        let Some(source) = source else {
            return 0;
        };
        encoder.copy_texture_to_texture(
            source.as_image_copy(),
            self.scratch.as_image_copy(),
            self.scratch.size(),
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("atmosphere RGB composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scene,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: self.timing.as_ref().map(|t| t.render()),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, &self.depth_bind_group, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        if let Some(timing) = &self.timing {
            timing.resolve(encoder, self.computed.get());
        }
        1
    }
    pub fn after_submit(&self) {
        if let Some(timing) = &self.timing {
            timing.after_submit();
        }
    }
    pub fn mark_environment(&self, environment: &crate::gpu_frame::EnvironmentUniform) {
        let key = [
            environment.global_sun_direction,
            environment.global_sun_rgb,
            environment.moon_orbit_world,
            environment.moon_disk_radiance,
            environment.ambient_rgb_exposure,
        ];
        if self.light_key.get() != Some(key) {
            self.light_key.set(Some(key));
            self.needs_lut.set(true);
        }
    }
    pub fn status(&self) -> Vec<String> {
        vec![format!("atmosphere={} quality={} height={:.0}m optical_radius={:.0}km memory={:.2}MiB debug={}",
            if self.active {"physical HDR"} else {"analytic fallback"},if self.standard {"standard"} else {"low"},
            self.last_view.up_height[3],self.last_view.radii[0]/1000.0,self.bytes() as f64/1048576.0,self.debug),
            self.timing.as_ref().map_or("GPU timestamp queries unsupported".into(),|t|t.status())]
    }
}
