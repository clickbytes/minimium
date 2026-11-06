use super::{sector_scene::ClientSectorScene, GpuResources};
use minimium_renderer::geometry::Vertex;
use std::{error::Error, sync::Arc};
use winit::{dpi::PhysicalSize, window::Window};

impl GpuResources {
    pub(super) fn cache_sector_scene(
        &mut self,
        instance: minimium_world::world::WorldInstanceId,
        scene: &ClientSectorScene,
    ) -> bool {
        if scene.origin != self.render_origin {
            return false;
        }
        let result = if let Some(prepared) = &scene.prepared {
            self.world_geometry.cache_prepared(
                &self.device,
                instance,
                scene.generation,
                scene.content_revision,
                &prepared.patches,
                if self.active_world_instance == instance {
                    &self.active_sector_keys
                } else {
                    &[]
                },
            )
        } else if scene.render_sectors.is_empty() {
            return false;
        } else {
            self.world_geometry.cache_scene(
                &self.device,
                instance,
                scene.generation,
                scene.content_revision,
                &scene.render_sectors,
                self.render_origin,
                None,
            )
        };
        self.fixed_gpu_bytes = self
            .fixed_gpu_bytes
            .saturating_sub(result.released_bytes)
            .saturating_add(result.allocated_bytes);
        result.ready
    }

    pub(super) fn activate_sector_scene(
        &mut self,
        instance: minimium_world::world::WorldInstanceId,
        scene: &ClientSectorScene,
    ) -> bool {
        if scene
            .render_sectors
            .iter()
            .any(|(key, lod, _)| !self.world_geometry.contains(instance, *key, *lod))
        {
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
        instance: minimium_world::world::WorldInstanceId,
    ) {
        self.fixed_gpu_bytes = self
            .fixed_gpu_bytes
            .saturating_sub(self.world_geometry.release_world(instance));
        if self.shadow_map_instance == Some(instance) {
            self.release_shadow_map_casters();
        }
    }

    pub(super) fn update_remote_vertices(&mut self, vertices: &[Vertex]) {
        self.remote_vertices.upload(&self.queue, vertices);
    }

    pub(super) fn set_render_origin(&mut self, origin: [f32; 3]) -> bool {
        if self.render_origin == origin {
            return false;
        }
        let delta = std::array::from_fn(|axis| self.render_origin[axis] - origin[axis]);
        self.render_origin = origin;
        if !self.world_geometry.rebase(&self.device, &self.queue, delta) {
            self.fixed_gpu_bytes = self
                .fixed_gpu_bytes
                .saturating_sub(self.world_geometry.clear());
            self.active_sector_keys.clear();
        }
        self.shadow_gpu.clear_player_casters();
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
        requested_quality: minimium_renderer::quality::SkyQuality,
    ) -> Result<Self, Box<dyn Error>> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await?;
        let adapter_info = adapter.get_info();
        eprintln!(
            "renderer_adapter name={:?} backend={:?} type={:?} preference=high_performance",
            adapter_info.name, adapter_info.backend, adapter_info.device_type
        );
        let shadow_usage =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let shadow_format =
            minimium_renderer::shadow_target::supported_format(&adapter, shadow_usage)
                .ok_or("graphics adapter has no sampleable depth texture format")?;
        let shadow_limit_supported = adapter.limits().max_texture_dimension_2d
            >= requested_quality.profile().shadow_resolution;
        let adapter_fallback_reason = (requested_quality.profile().shadows_enabled
            && (shadow_format != wgpu::TextureFormat::Depth32Float || !shadow_limit_supported))
            .then_some("adapter_limit_or_depth32float_unsupported");
        let initial_quality = if adapter_fallback_reason.is_some() {
            minimium_renderer::quality::SkyQuality::Low
        } else {
            requested_quality
        };
        let mut required_limits = wgpu::Limits::downlevel_defaults();
        required_limits.max_color_attachment_bytes_per_sample = adapter
            .limits()
            .max_color_attachment_bytes_per_sample
            .min(32);
        required_limits.max_texture_dimension_2d =
            adapter.limits().max_texture_dimension_2d.min(4096);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("minimium-device"),
                required_features: adapter.features()
                    & (wgpu::Features::TEXTURE_COMPRESSION_BC | wgpu::Features::TIMESTAMP_QUERY),
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
        let hdr_usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING;
        let hdr_enabled = adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
            && hdr_format.allowed_usages.contains(hdr_usage)
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
                minimium_renderer::OutputEncoder::new(
                    &device,
                    [size.width, size.height],
                    config.format,
                    scene_color_format,
                    hdr_enabled,
                )
            });
        let atmosphere_resources =
            minimium_renderer::atmosphere_resources::AtmosphereResources::new(
                &device,
                &queue,
                hdr_enabled,
            );
        let frame_bindings = minimium_renderer::gpu_bindings::EnvironmentGpuBindings::create(
            &device,
            &atmosphere_resources,
        );
        let milky_way = minimium_renderer::MilkyWayGpu::new(&device, &queue);
        let environment_pipelines = minimium_renderer::EnvironmentPipelines::new(
            &device,
            &frame_bindings.layout,
            &milky_way.layout,
            scene_color_format,
        );
        let celestial_halo = minimium_renderer::CelestialHaloGpu::new(
            &device,
            &frame_bindings.layout,
            scene_color_format,
        );
        let stars =
            minimium_renderer::StarGpu::new(&device, &frame_bindings.layout, scene_color_format);
        let moon =
            minimium_renderer::MoonGpu::new(&device, &frame_bindings.layout, scene_color_format);
        let moon_sphere = minimium_renderer::MoonSphereGpu::new(
            &device,
            &frame_bindings.layout,
            scene_color_format,
        );
        let depth_view = super::gpu_surface::create_depth_view(&device, size);
        let cloud_gpu = super::clouds::CloudGpu::new(
            &device,
            &adapter,
            &frame_bindings.layout,
            scene_color_format,
            &depth_view,
            [size.width, size.height],
        );
        eprintln!(
            "weather_transparency={} buffer_limit_bytes={} target_limit_bytes={}",
            if cloud_gpu.oit {
                "weighted_blended_oit"
            } else {
                "sorted_face_alpha"
            },
            minimium_renderer::CloudGpu::BUFFER_BYTES,
            minimium_renderer::cloud_targets::MAX_TARGET_BYTES,
        );
        let atmosphere_gpu = super::atmosphere_gpu::AtmosphereGpu::new(
            &device,
            &queue,
            &frame_bindings.layout,
            scene_color_format,
            &depth_view,
            atmosphere_resources,
        );
        let mut shadow_gpu = minimium_renderer::ShadowGpu::new(
            &device,
            minimium_renderer::ShadowSettings::for_quality(initial_quality),
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
        let pipeline = minimium_renderer::world_pipeline::create(
            &device,
            &pipeline_layout,
            scene_color_format,
        );
        let compressed = device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
            && device.limits().max_texture_dimension_2d >= 4096;
        let material_path = std::env::current_exe()?
            .parent()
            .ok_or("client executable has no directory")?
            .join("materials/sample-v1");
        let material_path = if material_path.exists() {
            material_path
        } else {
            std::path::PathBuf::from(env!("MINIMIUM_BUILT_MATERIALS"))
        };
        let package =
            minimium_asset_io::materials::MaterialPackage::open(&material_path, compressed)?;
        let terrain_gpu = minimium_renderer::terrain_gpu::TerrainGpu::new(
            &device,
            &queue,
            &frame_bindings.layout,
            &shadow_gpu.group2_layout,
            scene_color_format,
            &package.manifest,
            package.variant,
            &package.maps,
        )?;
        eprintln!(
            "terrain_materials resident_edge=2048 layers=6 gpu_mib={:.1}",
            terrain_gpu.bytes as f64 / 1048576.0
        );
        let ui = minimium_renderer::ui_pipeline::UiGpuResources::new(
            &device,
            &queue,
            scene_color_format,
            config.format,
            super::gpu_surface::is_srgb(config.format),
            hdr_enabled,
            &crate::console::font_atlas_pixels(),
            [
                crate::console::ATLAS_WIDTH as u32,
                crate::console::ATLAS_HEIGHT as u32,
            ],
            super::UI_VERTEX_CAPACITY,
        );
        let initial_world = minimium_renderer::geometry_gpu::StaticVertexBuffer::new(
            &device,
            "sample-world-geometry",
            vertices,
        );
        let remote_vertices = minimium_renderer::geometry_gpu::DynamicVertexBuffer::new(
            &device,
            "network-player-vertices",
            512 * 36,
        );
        let fixed_gpu_bytes = terrain_gpu.bytes
            + frame_bindings.camera_buffer.size()
            + frame_bindings.environment_buffer.size()
            + initial_world.size_bytes()
            + remote_vertices.size_bytes()
            + shadow_gpu.gpu_bytes()
            + ui.vertex_buffer.size()
            + stars.buffer.size()
            + minimium_renderer::MilkyWayGpu::BYTES
            + atmosphere_gpu.bytes()
            + cloud_gpu.bytes()
            + output_encoder
                .as_ref()
                .map_or(0, |output| output.size_bytes())
            + (crate::console::ATLAS_WIDTH * crate::console::ATLAS_HEIGHT) as u64;
        let world_geometry =
            minimium_renderer::world_geometry_gpu::WorldGeometryGpu::new(&device, hdr_enabled);
        Ok(Self {
            surface,
            device,
            queue,
            config,
            scene_color_format,
            hdr_enabled,
            pipeline,
            terrain_gpu,
            frame_layout: frame_bindings.layout,
            shadow_gpu,
            shadow_state: super::shadows::ShadowState::default(),
            environment_pipelines,
            milky_way,
            celestial_halo,
            stars,
            uploaded_star_stamp: None,
            moon,
            moon_sphere,
            atmosphere_gpu,
            cloud_gpu,
            ui,
            initial_world,
            remote_vertices,
            shadow_remote_complete: true,
            shadow_remote_dirty: false,
            camera_buffer: frame_bindings.camera_buffer,
            environment_buffer: frame_bindings.environment_buffer,
            camera_bind_group: frame_bindings.bind_group,
            depth_view,
            size,
            fixed_gpu_bytes,
            world_geometry,
            active_sector_keys: Vec::new(),
            active_world_instance: minimium_world::world::WorldInstanceId::new(1)
                .expect("primary world ID is nonzero"),
            render_origin: [0.0; 3],
            shadow_map_instance: None,
            shadow_map_fingerprint: None,
            shadow_map_center: None,
            shadow_map_radius: None,
            overview_buffers: minimium_renderer::overview_gpu::OverviewGpuBuffers::default(),
            environment_draw_calls: super::sky::EnvironmentDrawCalls::default(),
            last_shadow_prepare_cpu: None,
            last_environment_upload_cpu: None,
            overview_active: false,
            overview_planet: false,
            scene_visible: true,
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
        let old_atmosphere_bytes = self.atmosphere_gpu.bytes();
        self.atmosphere_gpu.resize(&self.device, &self.depth_view);
        self.fixed_gpu_bytes =
            self.fixed_gpu_bytes.saturating_sub(old_atmosphere_bytes) + self.atmosphere_gpu.bytes();
        let cloud_bytes = self.cloud_gpu.bytes();
        self.cloud_gpu
            .resize(&self.device, &self.depth_view, [size.width, size.height]);
        self.fixed_gpu_bytes = self
            .fixed_gpu_bytes
            .saturating_sub(cloud_bytes)
            .saturating_add(self.cloud_gpu.bytes());
        if let Some(output) = &mut self.output_encoder {
            let old_size = output.size_bytes();
            output.resize(&self.device, [size.width, size.height]);
            self.fixed_gpu_bytes = self
                .fixed_gpu_bytes
                .saturating_sub(old_size)
                .saturating_add(output.size_bytes());
        }
    }
}
