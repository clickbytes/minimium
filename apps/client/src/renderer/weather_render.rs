use super::GpuResources;

impl GpuResources {
    pub(super) fn render_weather(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        clouds: bool,
    ) -> u32 {
        if !self.scene_visible {
            return 0;
        }
        let mut calls = 0;
        if self.overview_active {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("orbital-atmosphere-limb"),
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
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.weather_gpu.atmosphere);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.weather_gpu.bind_group, &[]);
            pass.set_bind_group(2, &self.weather_gpu.depth_bind_group, &[]);
            pass.draw(0..3, 0..1);
            calls += 1;
        }
        if clouds {
            self.weather_gpu.timing.begin();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scaled-analytic-cloud-optical-bins"),
                color_attachments: &std::array::from_fn::<_, 4, _>(|i| {
                    let view = &self.weather_gpu.cloud_targets.views[i];
                    Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })
                }),
                depth_stencil_attachment: None,
                timestamp_writes: self.weather_gpu.timing.writes(true),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.weather_gpu.clouds);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.weather_gpu.bind_group, &[]);
            pass.set_bind_group(2, &self.weather_gpu.depth_bind_group, &[]);
            pass.set_bind_group(3, &self.weather_gpu.instances.bind_group, &[]);
            pass.draw(0..6, 0..self.weather_gpu.instances.draw_count);
            pass.set_pipeline(&self.weather_gpu.cloud_aggregates);
            pass.set_vertex_buffer(0, self.weather_gpu.instances.far.vertices.slice(..));
            pass.set_index_buffer(
                self.weather_gpu.instances.far.indices.slice(..),
                wgpu::IndexFormat::Uint32,
            );
            pass.draw_indexed(
                self.weather_gpu
                    .instances
                    .far
                    .draw_range(self.weather_gpu.cloud_effective.aggregates),
                0,
                0..1,
            );
            calls += 2;
        }
        if clouds {
            // Depth is sampled here, never simultaneously attached for writing.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("depth-aware-cloud-composite"),
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
                timestamp_writes: self.weather_gpu.timing.writes(false),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.weather_gpu.cloud_composite);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.weather_gpu.bind_group, &[]);
            pass.set_bind_group(2, &self.weather_gpu.cloud_targets.bind_group, &[]);
            pass.draw(0..3, 0..1);
            calls += 1;
        }
        if clouds {
            self.weather_gpu.timing.resolve(encoder);
        }
        let rain = clouds && !self.overview_active && self.weather_gpu.rain_mm_h > 0.05;
        if clouds && !self.overview_active {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("analytic-weather-fog"),
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
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.weather_gpu.fog);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.weather_gpu.bind_group, &[]);
            pass.set_bind_group(2, &self.weather_gpu.depth_bind_group, &[]);
            pass.draw(0..3, 0..1);
            calls += 1;
        }
        if rain || self.weather_gpu.bolt_active {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("weather-particles"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scene,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.weather_gpu.bind_group, &[]);
            if rain {
                pass.set_pipeline(&self.weather_gpu.rain);
                pass.draw(0..6, 0..self.weather_gpu.rain_particles);
                calls += 1;
            }
            if self.weather_gpu.bolt_active {
                pass.set_pipeline(&self.weather_gpu.bolt);
                pass.draw(0..6, 0..8);
                calls += 1;
            }
        }
        calls
    }
}
