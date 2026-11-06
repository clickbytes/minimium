use super::*;

impl Renderer {
    pub(super) fn prepare_session_materials(&mut self) {
        let Some(session) = self
            .connection
            .session_mut()
            .filter(|s| s.content_awaiting_gpu())
        else {
            return;
        };
        self.gpu.scene_visible = false;
        self.connection_status = "Preparing terrain materials".into();
        let compressed = self
            .gpu
            .device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
            && self.gpu.device.limits().max_texture_dimension_2d >= 4096;
        let package = session.take_material_package().map(Ok).unwrap_or_else(|| {
            let installed = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.join("materials/sample-v1")));
            let path = installed
                .filter(|p| p.exists())
                .unwrap_or_else(|| PathBuf::from(env!("MINIMIUM_BUILT_MATERIALS")));
            minimium_asset_io::materials::MaterialPackage::open(&path, compressed)
        });
        let result = package.and_then(|package| {
            self.gpu.terrain_gpu.release_materials();
            minimium_renderer::terrain_gpu::TerrainGpu::new(
                &self.gpu.device,
                &self.gpu.queue,
                &self.gpu.frame_layout,
                &self.gpu.shadow_gpu.group2_layout,
                self.gpu.scene_color_format,
                &package.manifest,
                package.variant,
                &package.maps,
            )
        });
        match result {
            Ok(mut resources) => {
                resources.uniform.debug = self.gpu.terrain_gpu.uniform.debug;
                self.gpu.fixed_gpu_bytes = self
                    .gpu
                    .fixed_gpu_bytes
                    .saturating_sub(self.gpu.terrain_gpu.bytes)
                    + resources.bytes;
                self.gpu.terrain_gpu = resources;
                if let Err(error) = session.finish_content() {
                    self.fail_connection(error.to_string());
                }
            }
            Err(error) => {
                self.fail_connection(format!("Terrain materials: {error}; use connect to retry"))
            }
        }
    }
}
