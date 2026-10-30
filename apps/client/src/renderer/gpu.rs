use super::{sector_scene::ClientSectorScene, GpuResources, Vertex};
use std::{error::Error, sync::Arc};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

impl GpuResources {
    pub(super) fn cache_sector_scene(
        &mut self,
        instance: minimium_core::world::WorldInstanceId,
        scene: &ClientSectorScene,
    ) -> bool {
        let version = (scene.generation, scene.content_revision);
        if self
            .world_sector_versions
            .get(&instance)
            .is_some_and(|cached| *cached != version)
        {
            self.release_world_sectors(instance);
        }
        self.world_sector_versions.insert(instance, version);
        let required: std::collections::HashSet<_> = scene
            .render_sectors
            .iter()
            .map(|(key, lod, _)| (*key, *lod))
            .collect();
        for (key, lod, geometry) in &scene.render_sectors {
            if self
                .world_vertex_catalog
                .contains_key(&(instance, *key, *lod))
            {
                continue;
            }
            let vertices = super::geometry::sector_geometry_relative(geometry, self.render_origin);
            let empty_vertices = [0.0_f32; 9];
            let contents = if vertices.is_empty() {
                bytemuck::cast_slice(&empty_vertices)
            } else {
                bytemuck::cast_slice(&vertices)
            };
            let buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("world-sector-geometry"),
                    contents,
                    usage: wgpu::BufferUsages::VERTEX,
                });
            self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_add(buffer.size());
            self.world_vertex_catalog
                .insert((instance, *key, *lod), (buffer, vertices.len() as u32));
            if let Some(bounds) = super::shadows::geometry_bounds(geometry) {
                self.world_geometry_bounds
                    .insert((instance, *key, *lod), bounds);
            }
        }
        self.world_vertex_catalog
            .retain(|(owner, key, lod), (buffer, _)| {
                if *owner != instance || required.contains(&(*key, *lod)) {
                    true
                } else {
                    self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_sub(buffer.size());
                    false
                }
            });
        self.world_geometry_bounds
            .retain(|(owner, key, lod), _| *owner != instance || required.contains(&(*key, *lod)));
        if scene.render_sectors.iter().any(|(key, lod, _)| {
            !self
                .world_vertex_catalog
                .contains_key(&(instance, *key, *lod))
        }) {
            return false;
        }
        true
    }

    pub(super) fn activate_sector_scene(
        &mut self,
        instance: minimium_core::world::WorldInstanceId,
        scene: &ClientSectorScene,
    ) -> bool {
        if scene.render_sectors.iter().any(|(key, lod, _)| {
            !self
                .world_vertex_catalog
                .contains_key(&(instance, *key, *lod))
        }) {
            return false;
        }
        self.active_sector_keys = scene
            .render_sectors
            .iter()
            .map(|(key, lod, _)| (*key, *lod))
            .collect();
        self.active_world_instance = instance;
        true
    }

    pub(super) fn release_world_sectors(
        &mut self,
        instance: minimium_core::world::WorldInstanceId,
    ) {
        self.world_vertex_catalog
            .retain(|(owner, _, _), (buffer, _)| {
                if *owner != instance {
                    true
                } else {
                    self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_sub(buffer.size());
                    false
                }
            });
        self.world_sector_versions.remove(&instance);
        self.world_geometry_bounds
            .retain(|(owner, _, _), _| *owner != instance);
        if self.shadow_map_instance == Some(instance) {
            self.release_shadow_map_casters();
        }
    }

    pub(super) fn update_remote_vertices(&mut self, vertices: &[Vertex]) {
        self.remote_vertex_count = vertices.len() as u32;
        if self.remote_vertex_count > 0 {
            self.queue
                .write_buffer(&self.player_buffer, 0, bytemuck::cast_slice(vertices));
        }
    }

    pub(super) fn set_render_origin(&mut self, origin: [f32; 3]) -> bool {
        if self.render_origin == origin {
            return false;
        }
        self.render_origin = origin;
        for (_, (buffer, _)) in self.world_vertex_catalog.drain() {
            self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_sub(buffer.size());
        }
        self.world_sector_versions.clear();
        self.world_geometry_bounds.clear();
        self.active_sector_keys.clear();
        self.shadow_player_vertex_count = 0;
        self.shadow_remote_complete = false;
        self.shadow_remote_dirty = true;
        self.release_shadow_map_casters();
        self.shadow_state
            .invalidate(super::shadows::ShadowInvalidReason::StaleContext);
        true
    }

    pub(super) async fn new(
        window: Arc<Window>,
        vertices: &[Vertex],
        requested_quality: crate::environment::quality::SkyQuality,
    ) -> Result<Self, Box<dyn Error>> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await?;
        let shadow_usage =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let shadow_format = super::shadow_target::supported_format(&adapter, shadow_usage)
            .ok_or("graphics adapter has no sampleable depth texture format")?;
        let shadow_limit_supported = adapter.limits().max_texture_dimension_2d
            >= requested_quality.profile().shadow_resolution;
        let adapter_fallback_reason = (requested_quality.profile().shadows_enabled
            && (shadow_format != wgpu::TextureFormat::Depth32Float || !shadow_limit_supported))
            .then_some("adapter_limit_or_depth32float_unsupported");
        let initial_quality = if adapter_fallback_reason.is_some() {
            crate::environment::quality::SkyQuality::Low
        } else {
            requested_quality
        };
        let mut required_limits = wgpu::Limits::downlevel_defaults();
        required_limits.max_texture_dimension_2d =
            adapter.limits().max_texture_dimension_2d.min(4096);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("minimium-device"),
                required_features: wgpu::Features::empty(),
                required_limits,
                experimental_features: Default::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await?;
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("graphics adapter does not support this surface")?;
        surface.configure(&device, &config);
        let hdr_format = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba16Float);
        let hdr_usage =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let hdr_enabled = hdr_format.allowed_usages.contains(hdr_usage)
            && hdr_format
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
            && adapter.limits().max_texture_dimension_2d >= size.width.max(size.height);
        if !hdr_enabled {
            eprintln!(
                "renderer_hdr mode=ldr fallback_reason=rgba16float_unavailable_or_surface_limit"
            );
        }
        let scene_color_format = if hdr_enabled {
            wgpu::TextureFormat::Rgba16Float
        } else if !super::gpu_surface::is_srgb(config.format) {
            wgpu::TextureFormat::Rgba8Unorm
        } else {
            config.format
        };
        let output_encoder =
            (hdr_enabled || !super::gpu_surface::is_srgb(config.format)).then(|| {
                super::output::OutputEncoder::new(
                    &device,
                    size,
                    config.format,
                    scene_color_format,
                    hdr_enabled,
                )
            });
        let frame_bindings = super::environment_gpu::EnvironmentGpuBindings::create(&device);
        let milky_way = super::milky_way_gpu::MilkyWayGpu::new(&device, &queue);
        let environment_pipelines = super::sky_gpu::EnvironmentPipelines::new(
            &device,
            &frame_bindings.layout,
            &milky_way.layout,
            scene_color_format,
        );
        let celestial_halo = super::celestial_halo_gpu::CelestialHaloGpu::new(
            &device,
            &frame_bindings.layout,
            scene_color_format,
        );
        let stars =
            super::stars_gpu::StarGpu::new(&device, &frame_bindings.layout, scene_color_format);
        let moon =
            super::moon_gpu::MoonGpu::new(&device, &frame_bindings.layout, scene_color_format);
        let moon_sphere = super::moon_sphere_gpu::MoonSphereGpu::new(
            &device,
            &frame_bindings.layout,
            scene_color_format,
        );
        let mut shadow_gpu = super::shadow_gpu::ShadowGpu::new(
            &device,
            super::shadows::ShadowSettings::for_quality(initial_quality),
            shadow_format,
        )?;
        if adapter_fallback_reason.is_some() {
            shadow_gpu.fallback_reason = adapter_fallback_reason;
        }
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("minimium-world-shadow-pipeline-layout"),
            bind_group_layouts: &[
                Some(&frame_bindings.layout),
                Some(&shadow_gpu.empty_layout),
                Some(&shadow_gpu.group2_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = super::world_pipeline::create(&device, &pipeline_layout, scene_color_format);
        let (ui_pipeline, ui_bind_group, ui_vertex_buffer, ui_bind_group_layout) =
            super::gpu_surface::create_ui_resources(&device, &queue, scene_color_format);
        let ui_output_pipeline = hdr_enabled.then(|| {
            super::gpu_surface::create_ui_output_pipeline(
                &device,
                &ui_bind_group_layout,
                config.format,
                super::gpu_surface::is_srgb(config.format),
            )
        });
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sample-world-geometry"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let player_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("network-player-vertices"),
            size: (512 * 36 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let depth_view = super::gpu_surface::create_depth_view(&device, size);
        let fixed_gpu_bytes = frame_bindings.camera_buffer.size()
            + frame_bindings.environment_buffer.size()
            + vertex_buffer.size()
            + player_buffer.size()
            + shadow_gpu.gpu_bytes()
            + ui_vertex_buffer.size()
            + stars.buffer.size()
            + super::milky_way_gpu::MilkyWayGpu::BYTES
            + output_encoder
                .as_ref()
                .map_or(0, |output| output.size_bytes())
            + (crate::console::ATLAS_WIDTH * crate::console::ATLAS_HEIGHT) as u64;
        Ok(Self {
            surface,
            device,
            queue,
            config,
            scene_color_format,
            hdr_enabled,
            pipeline,
            shadow_gpu,
            shadow_state: super::shadows::ShadowState::default(),
            environment_pipelines,
            milky_way,
            celestial_halo,
            stars,
            moon,
            moon_sphere,
            ui_pipeline,
            ui_output_pipeline,
            ui_bind_group,
            ui_vertex_buffer,
            ui_vertex_count: 0,
            vertex_buffer,
            vertex_count: vertices.len() as u32,
            player_buffer,
            remote_vertex_count: 0,
            shadow_player_buffer: None,
            shadow_player_vertex_count: 0,
            shadow_remote_complete: true,
            shadow_remote_dirty: false,
            camera_buffer: frame_bindings.camera_buffer,
            environment_buffer: frame_bindings.environment_buffer,
            camera_bind_group: frame_bindings.bind_group,
            depth_view,
            size,
            fixed_gpu_bytes,
            world_vertex_catalog: std::collections::HashMap::new(),
            world_sector_versions: std::collections::HashMap::new(),
            world_geometry_bounds: std::collections::HashMap::new(),
            active_sector_keys: Vec::new(),
            active_world_instance: minimium_core::world::WorldInstanceId::new(1)
                .expect("primary world ID is nonzero"),
            render_origin: [0.0; 3],
            shadow_map_buffer: None,
            shadow_map_instance: None,
            shadow_map_fingerprint: None,
            shadow_map_center: None,
            shadow_map_vertex_count: 0,
            overview_buffer: None,
            overview_marker_buffer: None,
            environment_draw_calls: super::sky::EnvironmentDrawCalls::default(),
            last_shadow_prepare_cpu: None,
            last_environment_upload_cpu: None,
            overview_active: false,
            overview_globe: false,
            scene_visible: true,
            globe: None,
            output_encoder,
        })
    }

    pub(super) fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.size = size;
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = super::gpu_surface::create_depth_view(&self.device, size);
        if let Some(output) = &mut self.output_encoder {
            let old_size = output.size_bytes();
            output.resize(&self.device, size);
            self.fixed_gpu_bytes = self
                .fixed_gpu_bytes
                .saturating_sub(old_size)
                .saturating_add(output.size_bytes());
        }
    }
}
