use super::{scene_bridge::RenderSceneSnapshot, GpuResources};

impl GpuResources {
    /// Keeps every GPU pass on the one immutable snapshot prepared for this frame.
    pub(super) fn render(
        &mut self,
        snapshot: &RenderSceneSnapshot<'_>,
    ) -> Result<u32, wgpu::CurrentSurfaceTexture> {
        self.environment_draw_calls = super::sky::EnvironmentDrawCalls::default();
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
        let publication = snapshot.publication.as_ref();
        let publication_stamp = publication.map(|publication| publication.stamp);
        let publication_keys =
            publication.map_or(&[][..], |publication| publication.keys.as_slice());
        self.shadow_upload_status = if snapshot.shadow.valid
            && snapshot.shadow.context == environment.context
            && snapshot.shadow.geometry_stamp == publication_stamp
            && snapshot.shadow.instance
                == render_frame.client.as_ref().map(|view| view.world_instance)
        {
            snapshot.shadow.map_casters.as_ref().map_or(
                super::shadow_gpu::ShadowUploadStatus::Failed,
                |map| {
                    if Some(map.key.stamp) != snapshot.shadow.geometry_stamp
                        || Some(map.key.generation) != snapshot.shadow.generation
                        || Some(map.key.content_revision) != snapshot.shadow.content_revision
                    {
                        super::shadow_gpu::ShadowUploadStatus::Failed
                    } else {
                        self.upload_prepared_shadow_map(map)
                    }
                },
            )
        } else {
            if snapshot.shadow.valid
                && (snapshot.shadow.geometry_stamp != publication_stamp
                    || snapshot.shadow.context != environment.context
                    || snapshot.shadow.instance
                        != render_frame.client.as_ref().map(|view| view.world_instance))
            {
                super::shadow_gpu::ShadowUploadStatus::StaleContext
            } else {
                super::shadow_gpu::ShadowUploadStatus::Pending
            }
        };
        self.shadow_publish_ready =
            self.shadow_upload_status == super::shadow_gpu::ShadowUploadStatus::Ready;
        if self.shadow_publish_ready {
            self.shadow_gpu
                .write_uniform(&self.queue, &snapshot.shadow.uniform);
        }
        let mut environment_upload = render_frame
            .presentation_environment(self.hdr_enabled, self.atmosphere_gpu.is_active());
        environment_upload.render_origin = [
            self.render_origin[0],
            self.render_origin[1],
            self.render_origin[2],
            0.0,
        ];
        if let Some((position, color)) = super::bolids::strongest_bolid_light(
            &snapshot.bolids,
            render_frame.view.camera_eye_world_m,
            self.atmosphere_gpu.optical_uniform(),
        ) {
            environment_upload.bolid_position_intensity = position;
            environment_upload.bolid_rgb = color;
            // Analytic/fallback surfaces retain authored ambient exposure.
            // Match the point flash to HDR night metering without changing it.
            environment_upload.bolid_rgb[3] = if self.hdr_enabled && self.atmosphere_gpu.is_active()
            {
                1.0
            } else {
                minimium_renderer::lighting::view_exposure(
                    render_frame.view,
                    environment_upload.global_sun_direction,
                    1.0,
                )
            };
            let incident = minimium_renderer::bolid_light::incident_luminance(
                position,
                color,
                render_frame.view.camera_eye_world_m,
                self.atmosphere_gpu.optical_uniform(),
            );
            // Preserve physical point-source power. Meter the receiving scene
            // instead of letting fixed night gain clip a nearby flash to white.
            let exposed_gain =
                environment_upload.ambient_rgb_exposure[3] * environment_upload.bolid_rgb[3];
            environment_upload.ambient_rgb_exposure[3] *=
                minimium_renderer::bolid_light::exposure_scale(incident, exposed_gain);
        }
        let meteors = snapshot.meteors();
        // One bounded snapshot feeds the shared finite-distance emission layer.
        let lights =
            minimium_renderer::transient_lighting::TransientLightUniform::from_frame(&meteors);
        self.transient_count = lights.count[0];
        self.transient_incident =
            minimium_renderer::transient_lighting::TransientLightUniform::observer_irradiance(
                &meteors,
            );
        self.queue
            .write_buffer(&self.transient_buffer, 0, bytemuck::bytes_of(&lights));
        self.meteors_status = meteors.render_status();
        self.milky_way.meteors.upload(&self.queue, &meteors);
        self.atmosphere_gpu.mark_environment(&environment_upload);
        self.terrain_gpu.uniform.eye_relative = std::array::from_fn(|i| {
            if i < 3 {
                (render_frame.view.camera_eye_world_m[i] - render_frame.view.render_origin_m[i])
                    as f32
            } else {
                0.0
            }
        });
        self.terrain_gpu.update(&self.queue);
        let client = render_frame
            .client
            .expect("application attaches ClientView before rendering");
        let upload_started = std::time::Instant::now();
        self.milky_way.upload_bolids(&self.queue, &snapshot.bolids);
        prepared.upload_stars(self);
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&render_frame.camera),
        );
        self.queue.write_buffer(
            &self.environment_buffer,
            0,
            bytemuck::bytes_of(&environment_upload),
        );
        if let Some(output) = &self.output_encoder {
            output.update_exposure(&self.queue, 1.0);
        }
        let ui_vertices = render_frame.ui_vertices();
        if !ui_vertices.is_empty() {
            self.queue.write_buffer(
                self.ui.vertex_buffer(),
                0,
                bytemuck::cast_slice(ui_vertices),
            );
        }
        let ui_vertex_count = ui_vertices.len() as u32;
        self.ui.set_vertex_count(ui_vertex_count);
        self.last_environment_upload_cpu = Some(upload_started.elapsed());
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
            surface_stars: super::sky::should_draw_stars(environment)
                || self.atmosphere_gpu.high_stars(),
            surface_sun: super::sky::should_draw_sun(environment)
                || self.atmosphere_gpu.is_active(),
            surface_moon: super::sky::should_draw_moon(environment)
                || self.atmosphere_gpu.is_active(),
        };
        let scene = minimium_renderer::scene_encoder::MainScenePass {
            color_view: scene_view,
            depth_view: &self.depth_view,
            clear_color: super::sky::clear_color(self.scene_visible, self.overview_active),
            camera: &self.camera_bind_group,
            world_pipeline: &self.pipeline,
            terrain_gpu: &self.terrain_gpu,
            terrain_catalog: self.world_geometry.terrain(),
            shadow_empty: &self.shadow_gpu.empty_bind_group,
            shadow_sampling: &self.shadow_gpu.sampling_bind_group,
            environment_pipelines: &self.environment_pipelines,
            milky_way: &self.milky_way,
            stars: &self.stars,
            celestial_halo: &self.celestial_halo,
            moon: &self.moon,
            moon_sphere: &self.moon_sphere,
            atmosphere: &self.atmosphere_gpu,
            overview_buffers: &self.overview_buffers,
            world_vertices: self.initial_world.buffer(),
            world_vertex_count: self.initial_world.vertex_count(),
            remote_vertices: self.remote_vertices.buffer(),
            remote_vertex_count: self.remote_vertices.vertex_count(),
            world_vertex_catalog: self.world_geometry.catalog(),
            instance: publication_stamp.map_or(client.world_instance, |stamp| stamp.instance),
            sector_hints: publication_keys,
            scene_visible: self.scene_visible
                && self.terrain_gpu.valid
                && publication_stamp.is_some_and(|stamp| stamp.instance == client.world_instance),
            overview_active: self.overview_active,
            overview_planet: self.overview_planet,
            uploaded_stars_match: self.uploaded_star_stamp == environment.diagnostics.star_frame,
            halo_count: render_frame.halo_count,
            environment: environment_flags,
        };
        let encoded = minimium_renderer::frame_encoder::encode_frame(
            &mut encoder,
            minimium_renderer::frame_encoder::FramePasses {
                shadow: minimium_renderer::frame_encoder::ShadowPass {
                    gpu: &self.shadow_gpu,
                    enabled: self.shadow_publish_ready
                        && publication_stamp.is_some()
                        && snapshot.shadow.geometry_stamp == publication_stamp,
                    instance: client.world_instance,
                    keys: &snapshot.shadow.caster_keys,
                    world_vertices: self.world_geometry.catalog(),
                    map: self.shadow_gpu.map_casters(),
                    player: self.shadow_gpu.player_casters(),
                },
                scene,
                clouds: &self.cloud_gpu,
                final_passes: minimium_renderer::frame_encoder::FinalPasses {
                    scene_view,
                    surface_view,
                    ui_pipeline: self.ui.pipeline(),
                    ui_output_pipeline: self.ui.output_pipeline(),
                    ui_bind_group: self.ui.bind_group(),
                    ui_vertex_buffer: self.ui.vertex_buffer(),
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
        self.atmosphere_gpu.after_submit();
        Ok(draw_calls)
    }
}
