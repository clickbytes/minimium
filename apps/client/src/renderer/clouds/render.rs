use super::super::GpuResources;

fn attachment(
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

impl GpuResources {
    pub(in crate::renderer) fn render_clouds(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
    ) -> u32 {
        let cloud = &self.cloud_gpu;
        if !self.scene_visible || (cloud.count == 0 && !cloud.debug) {
            return 0;
        }
        let mut draws = 0;
        if cloud.count > 0 {
            let attachments = if let Some(t) = &cloud.targets {
                vec![
                    attachment(&t.accum, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)),
                    attachment(&t.reveal, wgpu::LoadOp::Clear(wgpu::Color::WHITE)),
                ]
            } else {
                vec![attachment(scene, wgpu::LoadOp::Load)]
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("regional cube clouds"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&cloud.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &cloud.group, &[]);
            pass.set_vertex_buffer(0, cloud.faces.slice(..));
            pass.draw(0..6, 0..cloud.count);
            draws += 1;
        }
        if cloud.count > 0 {
            if let (Some(pipeline), Some(targets)) = (&cloud.composite_pipeline, &cloud.targets) {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("cloud transparency composite"),
                    color_attachments: &[attachment(scene, wgpu::LoadOp::Load)],
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
        if cloud.debug {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("weather regions and wind"),
                color_attachments: &[attachment(scene, wgpu::LoadOp::Load)],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&cloud.debug_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &cloud.group, &[]);
            pass.draw(0..3, 0..1);
            draws += 1;
        }
        draws
    }
}
