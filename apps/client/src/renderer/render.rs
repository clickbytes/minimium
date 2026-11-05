use super::{scene_bridge::RenderSceneSnapshot, GpuResources};

impl GpuResources {
    /// Keeps every GPU pass on the one immutable snapshot prepared for this frame.
    pub(super) fn render(
        &mut self,
        snapshot: &RenderSceneSnapshot<'_>,
    ) -> Result<u32, wgpu::CurrentSurfaceTexture> {
        self.environment_draw_calls = super::sky::EnvironmentDrawCalls::default();
        self.last_shadow_prepare_cpu = None;
        self.last_environment_upload_cpu = None;
        let frame = match minimium_renderer::surface_frame::acquire(
            &self.surface,
            &self.device,
            &self.config,
        ) {
            minimium_renderer::surface_frame::SurfaceAcquire::Ready(frame) => frame,
            minimium_renderer::surface_frame::SurfaceAcquire::Skipped => return Ok(0),
            minimium_renderer::surface_frame::SurfaceAcquire::Retry(error) => return Err(error),
        };
        let surface_view = &frame.view;
        let prepared = snapshot.environment;
        let environment = &prepared.frame;
        let render_frame = &prepared.render_frame;
        let client = render_frame
            .client
            .expect("application attaches ClientView before rendering");
        let upload_started = std::time::Instant::now();
        prepared.upload_stars(self);
        let ui_vertex_count = render_frame.upload(
            &self.queue,
            &self.camera_buffer,
            &self.environment_buffer,
            &self.ui.vertex_buffer,
            self.hdr_enabled,
            self.output_encoder.as_ref(),
        );
        self.ui.set_vertex_count(ui_vertex_count);
        self.last_environment_upload_cpu = Some(upload_started.elapsed());
        // Includes local map caster selection/build/allocation/upload and shadow
        // uniform upload. Remote-player preparation occurs earlier in update().
        // This scope is disjoint from environment preparation and upload.
        let shadow_started = std::time::Instant::now();
        self.prepare_shadow_frame(environment, snapshot.map, client.player_position_m);
        self.last_shadow_prepare_cpu = Some(shadow_started.elapsed());
        let scene_view = self
            .output_encoder
            .as_ref()
            .map_or(surface_view, |output| output.target_view());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame-encoder"),
            });
        let environment_flags = minimium_renderer::scene_encoder::EnvironmentDrawFlags {
            overview_sky: super::sky::should_draw_overview_sky(environment),
            overview_stars: super::sky::should_draw_overview_stars(environment),
            overview_sun: super::sky::should_draw_overview_sun(environment),
            overview_moon: super::sky::should_draw_overview_moon(environment),
            surface_sky: super::sky::should_draw_surface_sky(environment),
            surface_stars: super::sky::should_draw_stars(environment),
            surface_sun: super::sky::should_draw_sun(environment),
            surface_moon: super::sky::should_draw_moon(environment),
        };
        let scene = minimium_renderer::scene_encoder::MainScenePass {
            color_view: scene_view,
            depth_view: &self.depth_view,
            clear_color: super::sky::clear_color(self.scene_visible, self.overview_active),
            camera: &self.camera_bind_group,
            world_pipeline: &self.pipeline,
            shadow_empty: &self.shadow_gpu.empty_bind_group,
            shadow_sampling: &self.shadow_gpu.sampling_bind_group,
            environment_pipelines: &self.environment_pipelines,
            milky_way: &self.milky_way,
            stars: &self.stars,
            celestial_halo: &self.celestial_halo,
            moon: &self.moon,
            moon_sphere: &self.moon_sphere,
            atmosphere: &self.atmosphere_gpu,
            globe: self.globe.as_ref(),
            overview_buffers: &self.overview_buffers,
            world_vertices: self.initial_world.buffer(),
            world_vertex_count: self.initial_world.vertex_count(),
            remote_vertices: self.remote_vertices.buffer(),
            remote_vertex_count: self.remote_vertices.vertex_count(),
            world_vertex_catalog: self.world_geometry.catalog(),
            instance: client.world_instance,
            sector_hints: &snapshot.sector_hints,
            scene_visible: self.scene_visible,
            overview_active: self.overview_active,
            overview_globe: self.overview_globe,
            uploaded_stars_match: self.uploaded_star_stamp == environment.diagnostics.star_frame,
            halo_count: render_frame.halo_count,
            environment: environment_flags,
        };
        let encoded = minimium_renderer::frame_encoder::encode_frame(
            &mut encoder,
            minimium_renderer::frame_encoder::FramePasses {
                shadow: minimium_renderer::frame_encoder::ShadowPass {
                    gpu: &self.shadow_gpu,
                    enabled: self.shadow_state.valid,
                    instance: client.world_instance,
                    keys: &self.shadow_state.caster_keys,
                    world_vertices: self.world_geometry.catalog(),
                    map: self.shadow_gpu.map_casters(),
                    player: self.shadow_gpu.player_casters(),
                },
                scene,
                clouds: &self.cloud_gpu,
                final_passes: minimium_renderer::frame_encoder::FinalPasses {
                    scene_view,
                    surface_view,
                    ui_pipeline: &self.ui.pipeline,
                    ui_output_pipeline: self.ui.output_pipeline.as_ref(),
                    ui_bind_group: &self.ui.bind_group,
                    ui_vertex_buffer: &self.ui.vertex_buffer,
                    ui_vertex_count: self.ui.vertex_count(),
                    output: self.output_encoder.as_ref(),
                },
            },
        );
        self.environment_draw_calls.shadows += encoded.shadow;
        self.environment_draw_calls.sky += encoded.scene.sky;
        self.environment_draw_calls.stars += encoded.scene.stars;
        self.environment_draw_calls.halos += encoded.scene.halos;
        self.environment_draw_calls.sun += encoded.scene.sun;
        self.environment_draw_calls.moon += encoded.scene.moon;
        self.environment_draw_calls.overview_marker += encoded.scene.overview_marker;
        let draw_calls = encoded.shadow
            + encoded.scene.draws
            + encoded.atmosphere
            + encoded.clouds
            + encoded.final_passes;
        frame.submit(&self.queue, encoder);
        Ok(draw_calls)
    }
}
