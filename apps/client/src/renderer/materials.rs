use super::*;

pub(super) fn open_offline_materials(
    compressed: bool,
) -> Result<minimium_asset_io::materials::MaterialPackage, minimium_asset_io::AssetIoError> {
    use minimium_asset_io::AssetIoError;
    let manifest = match std::env::var_os("MINIMIUM_MATERIAL_MANIFEST") {
        Some(path) => PathBuf::from(path),
        None => {
            let executable = std::env::current_exe()
                .map_err(|error| AssetIoError::io("locate client material manifest", error))?;
            executable
                .parent()
                .ok_or_else(|| {
                    AssetIoError::invalid_format(
                        "client material manifest",
                        "no executable directory",
                    )
                })?
                .join("materials/manifest.ron")
        }
    };
    minimium_asset_io::materials::MaterialPackage::open_manifest(&manifest, compressed)
}

impl DesktopHost {
    pub(super) fn prepare_session_materials(&mut self) {
        let package = match self
            .application
            .take_material_package_if_awaiting_gpu(crate::host_time::sample())
        {
            Ok(Some(package)) => package,
            Ok(None) => return,
            Err(error) => {
                self.application
                    .record_diagnostic_failure("client.invalid_time", &error);
                return;
            }
        };
        self.renderer.scene_visible = false;
        self.application.status = crate::application::ClientStatus::Status(
            minimium_localization::UiStatus::TerrainMaterials,
        );
        let compressed = self
            .renderer
            .device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
            && self.renderer.device.limits().max_texture_dimension_2d >= 4096;
        let package = package.map(Ok).unwrap_or_else(|| {
            open_offline_materials(compressed).map_err(|error| error.to_string())
        });
        let result = package.and_then(|package| {
            self.renderer.terrain_gpu.release_materials();
            minimium_renderer::terrain_gpu::TerrainGpu::new(
                &self.renderer.device,
                &self.renderer.queue,
                &self.renderer.frame_layout,
                &self.renderer.shadow_gpu.group2_layout,
                self.renderer.scene_color_format,
                &package.manifest,
                package.variant,
                &package.maps,
            )
        });
        match result {
            Ok(mut resources) => {
                resources.uniform.debug = self.renderer.terrain_gpu.uniform.debug;
                self.renderer.fixed_gpu_bytes = self
                    .renderer
                    .fixed_gpu_bytes
                    .saturating_sub(self.renderer.terrain_gpu.bytes)
                    + resources.bytes;
                self.renderer.terrain_gpu = resources;
                if let Some(Err(error)) = self
                    .application
                    .finish_material_content(crate::host_time::sample())
                {
                    self.fail_connection(error.to_string());
                }
            }
            Err(error) => {
                self.fail_connection(format!("Terrain materials: {error}; use connect to retry"))
            }
        }
    }
}
