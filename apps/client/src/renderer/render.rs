use super::{scene_bridge::RenderSceneSnapshot, Renderer};

#[derive(Clone, Copy)]
pub(super) struct ExposureHistory {
    context: minimium_protocol::WorldContext,
    view: minimium_renderer::EnvironmentViewKind,
    clock: Option<minimium_environment::time::ClockRevision>,
    sampled_at: std::time::Instant,
    world_utc_ms: i64,
}

impl Renderer {
    /// Keeps every GPU pass on the one immutable snapshot prepared for this frame.
    pub(super) fn render(
        &mut self,
        snapshot: &RenderSceneSnapshot<'_>,
        submerged_fraction: f32,
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
        let mut disabled = minimium_renderer::shadow_uniform::ShadowUniform::default();
        disabled.flags[3] = u32::from(self.shadow_gpu.settings.enabled);
        let (local, landscape) = if self.shadow_publish_ready {
            (
                &snapshot.shadow.uniform,
                snapshot.shadow.landscape_uniform.as_ref(),
            )
        } else {
            (&disabled, None)
        };
        let moon = snapshot
            .shadow
            .secondary_moon
            .each_ref()
            .map(|v| v.as_ref().filter(|_| self.shadow_publish_ready));
        self.shadow_gpu
            .write_cascades(&self.queue, local, landscape, moon);
        self.atmosphere_gpu.mark_shadows(
            publication_stamp,
            self.shadow_gpu.caster_revision(),
            local,
            landscape,
            moon,
        );
        let mut environment_upload = render_frame
            .presentation_environment(self.hdr_enabled, self.atmosphere_gpu.is_active());
        // Fixed gain applies before the HDR storage conversion. The analytic route
        // also removes its observer-dependent lunar gain for a reproducible comparison.
        if let Some(ev) = self.fixed_exposure_ev {
            if !self.atmosphere_gpu.is_active() || !self.hdr_enabled {
                environment_upload.ambient_rgb_exposure[3] = ev.exp2();
                environment_upload.star_animation[2] = 1.0;
            }
        }
        let (stars, galaxy) = minimium_renderer::atmosphere_view::celestial_visibility(
            &environment_upload,
            self.atmosphere_gpu.optical_uniform(),
        );
        environment_upload.sky_horizon_rgb_star_visibility[3] = stars;
        environment_upload.star_animation[3] = galaxy;
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
                minimium_renderer::render_frame::view_exposure(
                    render_frame.view,
                    environment_upload.global_sun_direction,
                    1.0,
                )
            };
            // A flash adds incident light; it must not turn down terrain, sky
            // and the atmosphere LUTs globally. The output shoulder handles HDR.
        }
        let meteors = snapshot.meteors();
        let light_frame = &render_frame.transient_lights;
        // One bounded snapshot feeds the shared finite-distance emission layer.
        let mut lights =
            minimium_renderer::transient_lighting::TransientLightUniform::from_frame(light_frame);
        if !self.transient_enabled {
            lights.count[0] = 0;
        }
        let cloud_revision = self.cloud_gpu.revision().unwrap_or_default();
        self.cloud_gpu.light_cache.attach(
            &mut lights,
            &self.cloud_gpu.light_records,
            minimium_renderer::transient_cloud_cache::CloudLightQuery {
                ids: std::array::from_fn(|i| {
                    light_frame.lights().get(i).map_or(0, |light| light.id)
                }),
                origin: render_frame.view.render_origin_m,
                revision: cloud_revision,
                now: std::time::Instant::now(),
                eye: std::array::from_fn(|i| {
                    (render_frame.view.camera_eye_world_m[i] - render_frame.view.render_origin_m[i])
                        as f32
                }),
                standard: self.cloud_gpu.standard_lighting && light_frame.cloud_occlusion(),
            },
        );
        lights.count[1] = u32::from(self.transient_transmission);
        self.transient_count = lights.count[0];
        self.transient_incident =
            minimium_renderer::transient_lighting::TransientLightUniform::observer_irradiance(
                light_frame,
            );
        if !self.transient_enabled {
            self.transient_incident = 0.0;
        }
        self.queue
            .write_buffer(&self.transient_buffer, 0, bytemuck::bytes_of(&lights));
        self.meteors_status = meteors.render_status();
        self.milky_way
            .meteors
            .upload(&self.queue, &meteors, light_frame, &render_frame.view);
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
            .expect("desktop host attaches ClientView before rendering");
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
        let scene_visible = self.scene_visible
            && self.terrain_gpu.valid
            && publication_stamp.is_some_and(|stamp| stamp.instance == client.world_instance);
        if let Some(output) = &self.output_encoder {
            let context = snapshot.context.filter(|_| {
                self.hdr_enabled
                    && self.atmosphere_gpu.is_active()
                    && scene_visible
                    && self.terrain_gpu.uniform.debug[0] == 0
                    && self.atmosphere_gpu.debug == 0
                    && !self.cloud_gpu.debug
                    && !self.transient_transmission
            });
            let fixed_gain = self.fixed_exposure_ev.map(|ev| {
                if self.hdr_enabled && self.atmosphere_gpu.is_active() {
                    ev.exp2() / environment_upload.ambient_rgb_exposure[3].max(1e-5)
                } else {
                    1.0
                }
            });
            if let (Some(context), Some(sky)) = (context, environment.sky.as_ref()) {
                let now = std::time::Instant::now();
                let view = render_frame.view;
                let history = ExposureHistory {
                    context,
                    view: view.kind,
                    clock: environment.clock_revision,
                    sampled_at: now,
                    world_utc_ms: sky.utc_ms.get(),
                };
                let cadence = self
                    .exposure_history
                    .replace(history)
                    .filter(|old| {
                        old.context == history.context
                            && old.view == history.view
                            && old.clock == history.clock
                    })
                    .and_then(|old| {
                        let real = now.saturating_duration_since(old.sampled_at);
                        let world_ms = history.world_utc_ms.saturating_sub(old.world_utc_ms);
                        (world_ms >= 0 && real.as_secs_f32() < 1.0).then(|| {
                            minimium_renderer::output::ExposureInterval {
                                real,
                                world: std::time::Duration::from_millis(world_ms as u64),
                            }
                        })
                    });
                let base = render_frame.environment.ambient_rgb_exposure[3];
                let reference = minimium_renderer::render_frame::exposure_reference(
                    view,
                    environment_upload.global_sun_direction,
                    base,
                );
                output.meter_exposure_with_water(
                    &self.queue,
                    environment_upload.ambient_rgb_exposure[3],
                    base,
                    reference,
                    cadence,
                    submerged_fraction,
                    fixed_gain,
                );
            } else {
                self.exposure_history = None;
                output.update_exposure_with_water(
                    &self.queue,
                    fixed_gain.unwrap_or(1.0),
                    submerged_fraction,
                );
            }
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
        // A finite disk can peek over a dipped or refracted horizon on either GPU tier.
        let planetary_surface_sky = super::sky::should_draw_surface_sky(environment)
            && self.atmosphere_gpu.optical_uniform().config[0] > 0.0;
        let environment_flags = minimium_renderer::scene_encoder::EnvironmentDrawFlags {
            overview_sky: super::sky::should_draw_overview_sky(environment),
            overview_stars: super::sky::should_draw_overview_stars(environment),
            overview_sun: super::sky::should_draw_overview_sun(environment),
            overview_moon: super::sky::should_draw_overview_moon(environment),
            surface_sky: super::sky::should_draw_surface_sky(environment),
            surface_stars: super::sky::should_draw_stars(environment, stars),
            surface_sun: super::sky::should_draw_sun(environment) || planetary_surface_sky,
            surface_moon: super::sky::should_draw_moon(environment) || planetary_surface_sky,
        };
        let instance = publication_stamp.map_or(client.world_instance, |stamp| stamp.instance);
        let (local_water_buffer, local_water_vertex_count) =
            self.world_geometry.local_water(instance);
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
            local_water_buffer,
            local_water_vertex_count,
            remote_vertices: self.remote_vertices.buffer(),
            remote_vertex_count: self.remote_vertices.vertex_count(),
            remote_publication_matches: self.remote_stamp.is_some_and(|stamp| {
                Some(stamp.context) == snapshot.context
                    && Some(stamp.publication) == publication_stamp
            }),
            world_vertex_catalog: self.world_geometry.catalog(),
            instance,
            sector_hints: publication_keys,
            scene_visible,
            overview_active: self.overview_active,
            overview_planet: self.overview_planet,
            uploaded_stars_match: self.uploaded_star_stamp == environment.diagnostics.star_frame,
            halo_count: render_frame.halo_count,
            environment: environment_flags,
        };
        let held_tool = if self.scene_visible && self.held_tool_visible {
            self.held_tool_model_index
                .map(|model_index| (&self.held_tool, model_index))
        } else {
            None
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
                    secondary_moon: std::array::from_fn(|i| {
                        moon[i].map(|_| snapshot.shadow.moon_keys[i].as_slice())
                    }),
                    landscape_keys: landscape
                        .map(|_| snapshot.shadow.landscape_caster_keys.as_slice()),
                    world_vertices: self.world_geometry.catalog(),
                    map: self.shadow_gpu.map_casters(),
                    player: self.shadow_gpu.player_casters(),
                },
                scene,
                clouds: &self.cloud_gpu,
                held_tool,
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
            + encoded.held_tool
            + encoded.final_passes;
        frame.submit(&self.queue, encoder);
        self.atmosphere_gpu.after_submit();
        Ok(draw_calls)
    }
}
