use super::super::GpuResources;

impl GpuResources {
    pub(in crate::renderer) fn render_atmosphere(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
    ) -> u32 {
        if !self.scene_visible
            || !self.overview_active
            || !self.overview_globe
            || self.globe.is_none()
        {
            return 0;
        }
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
        pass.set_pipeline(&self.atmosphere_gpu.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_bind_group(1, &self.atmosphere_gpu.bind_group, &[]);
        pass.set_bind_group(2, &self.atmosphere_gpu.depth_bind_group, &[]);
        pass.draw(0..3, 0..1);
        1
    }
}
