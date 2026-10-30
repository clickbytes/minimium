use super::environment::EnvironmentFrame;
use super::GpuResources;

impl GpuResources {
    /// Keeps every GPU pass on the one immutable snapshot prepared for this frame.
    pub(super) fn render(
        &mut self,
        environment: &EnvironmentFrame,
        map: Option<&minimium_core::world::WorldMap>,
        player_position_m: [f64; 3],
    ) -> Result<u32, wgpu::CurrentSurfaceTexture> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.surface.configure(&self.device, &self.config);
                frame
            }
            error @ (wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost) => {
                return Err(error);
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(0);
            }
            error @ wgpu::CurrentSurfaceTexture::Validation => return Err(error),
        };
        let surface_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.prepare_shadow_frame(environment, map, player_position_m);
        let scene_view = self
            .output_encoder
            .as_ref()
            .map_or(&surface_view, |output| output.target_view());
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&environment.camera_uniform),
        );
        self.queue.write_buffer(
            &self.environment_buffer,
            0,
            bytemuck::bytes_of(&super::environment_gpu::environment_uniform(environment)),
        );
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame-encoder"),
            });
        let mut draw_calls = 0;
        if self.shadow_state.valid {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("local-shadow-depth-pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_gpu.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow_gpu.depth_pipeline);
            pass.set_bind_group(0, &self.shadow_gpu.uniform_bind_group, &[]);
            for (key, lod) in &self.shadow_state.caster_keys {
                if let Some((buffer, vertex_count)) =
                    self.world_vertex_catalog
                        .get(&(self.active_world_instance, *key, *lod))
                {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..*vertex_count, 0..1);
                    draw_calls += 1;
                }
            }
            if let Some(buffer) = &self.shadow_map_buffer {
                if self.shadow_map_vertex_count > 0 {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..self.shadow_map_vertex_count, 0..1);
                    draw_calls += 1;
                }
            }
            if self.shadow_player_vertex_count > 0 {
                if let Some(buffer) = &self.shadow_player_buffer {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..self.shadow_player_vertex_count, 0..1);
                    draw_calls += 1;
                }
            }
        }
        self.environment_draw_calls = super::sky::EnvironmentDrawCalls::default();
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scene_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(super::sky::clear_color(
                            self.scene_visible,
                            self.overview_active,
                        )),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            if !self.scene_visible {
                // Waiting for a required Earth manifest or showing a persistent error.
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(1, &self.shadow_gpu.empty_bind_group, &[]);
                pass.set_bind_group(2, &self.shadow_gpu.sampling_bind_group, &[]);
            } else if self.overview_active {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(1, &self.shadow_gpu.empty_bind_group, &[]);
                pass.set_bind_group(2, &self.shadow_gpu.sampling_bind_group, &[]);
                if let Some(globe) = self.globe.as_ref().filter(|_| self.overview_globe) {
                    pass.set_pipeline(&globe.pipeline);
                    pass.set_bind_group(1, &globe.bind_group, &[]);
                    pass.set_vertex_buffer(0, globe.vertices.slice(..));
                    pass.draw(0..globe.vertex_count, 0..1);
                    draw_calls += 1;
                    if let Some(detail) = &globe.detail {
                        pass.set_bind_group(1, &detail.bind_group, &[]);
                        pass.set_vertex_buffer(0, detail.vertices.slice(..));
                        pass.draw(0..detail.vertex_count, 0..1);
                        draw_calls += 1;
                    }
                    pass.set_pipeline(&self.pipeline);
                    pass.set_bind_group(1, &self.shadow_gpu.empty_bind_group, &[]);
                    pass.set_bind_group(2, &self.shadow_gpu.sampling_bind_group, &[]);
                }
                if let Some((buffer, count)) = &self.overview_buffer {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..*count, 0..1);
                    draw_calls += 1;
                }
                if let Some((buffer, count)) = &self.overview_marker_buffer {
                    pass.set_pipeline(&self.environment_pipelines.marker);
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..*count, 0..1);
                    draw_calls += 1;
                    self.environment_draw_calls.overview_marker += 1;
                }
            } else if self.active_sector_keys.is_empty() {
                if super::sky::should_draw_surface_sky(environment) {
                    pass.set_pipeline(&self.environment_pipelines.sky);
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    pass.draw(0..3, 0..1);
                    draw_calls += 1;
                    self.environment_draw_calls.sky += 1;
                    if super::sky::should_draw_stars(environment)
                        && self.stars.matches_frame(environment)
                    {
                        pass.set_pipeline(&self.stars.pipeline);
                        pass.set_vertex_buffer(0, self.stars.buffer.slice(..));
                        pass.draw(
                            0..6,
                            0..super::environment::stars::STAR_GPU_RECORD_COUNT as u32,
                        );
                        draw_calls += 1;
                        self.environment_draw_calls.stars += 1;
                    }
                    if super::sky::should_draw_sun(environment) {
                        pass.set_pipeline(&self.environment_pipelines.sun);
                        pass.draw(0..6, 0..1);
                        draw_calls += 1;
                        self.environment_draw_calls.sun += 1;
                    }
                    if super::sky::should_draw_moon(environment) {
                        pass.set_pipeline(&self.moon.pipeline);
                        pass.draw(0..6, 0..1);
                        draw_calls += 1;
                        self.environment_draw_calls.moon += 1;
                    }
                }
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(1, &self.shadow_gpu.empty_bind_group, &[]);
                pass.set_bind_group(2, &self.shadow_gpu.sampling_bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.draw(0..self.vertex_count, 0..1);
                draw_calls += 1;
            } else {
                if super::sky::should_draw_surface_sky(environment) {
                    pass.set_pipeline(&self.environment_pipelines.sky);
                    pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    pass.draw(0..3, 0..1);
                    draw_calls += 1;
                    self.environment_draw_calls.sky += 1;
                    if super::sky::should_draw_stars(environment)
                        && self.stars.matches_frame(environment)
                    {
                        pass.set_pipeline(&self.stars.pipeline);
                        pass.set_vertex_buffer(0, self.stars.buffer.slice(..));
                        pass.draw(
                            0..6,
                            0..super::environment::stars::STAR_GPU_RECORD_COUNT as u32,
                        );
                        draw_calls += 1;
                        self.environment_draw_calls.stars += 1;
                    }
                    if super::sky::should_draw_sun(environment) {
                        pass.set_pipeline(&self.environment_pipelines.sun);
                        pass.draw(0..6, 0..1);
                        draw_calls += 1;
                        self.environment_draw_calls.sun += 1;
                    }
                    if super::sky::should_draw_moon(environment) {
                        pass.set_pipeline(&self.moon.pipeline);
                        pass.draw(0..6, 0..1);
                        draw_calls += 1;
                        self.environment_draw_calls.moon += 1;
                    }
                }
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(1, &self.shadow_gpu.empty_bind_group, &[]);
                pass.set_bind_group(2, &self.shadow_gpu.sampling_bind_group, &[]);
                for (key, lod) in &self.active_sector_keys {
                    if let Some((buffer, vertex_count)) =
                        self.world_vertex_catalog
                            .get(&(self.active_world_instance, *key, *lod))
                    {
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        pass.draw(0..*vertex_count, 0..1);
                        draw_calls += 1;
                    }
                }
            }
            if !self.overview_active && self.remote_vertex_count > 0 {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(1, &self.shadow_gpu.empty_bind_group, &[]);
                pass.set_bind_group(2, &self.shadow_gpu.sampling_bind_group, &[]);
                pass.set_vertex_buffer(0, self.player_buffer.slice(..));
                pass.draw(0..self.remote_vertex_count, 0..1);
                draw_calls += 1;
            }
        }
        if self.ui_vertex_count > 0 {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("minimium-console-ui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scene_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.ui_pipeline);
            pass.set_bind_group(0, &self.ui_bind_group, &[]);
            pass.set_vertex_buffer(0, self.ui_vertex_buffer.slice(..));
            pass.draw(0..self.ui_vertex_count, 0..1);
            draw_calls += 1;
        }
        if let Some(output) = &self.output_encoder {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("linear-output-srgb-encode-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(output.pipeline());
            pass.set_bind_group(0, output.bind_group(), &[]);
            pass.draw(0..3, 0..1);
            draw_calls += 1;
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        Ok(draw_calls)
    }
}
