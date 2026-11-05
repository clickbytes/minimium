use super::Renderer;
pub(super) use minimium_renderer::globe_detail::Region;
use std::sync::mpsc::TryRecvError;

impl Renderer {
    fn clear_globe_detail(&mut self) {
        if let Some(globe) = self.gpu.globe.as_mut() {
            self.gpu.fixed_gpu_bytes = self
                .gpu
                .fixed_gpu_bytes
                .saturating_sub(globe.clear_detail());
        }
    }

    fn install_globe_detail(&mut self, pixels: &[u8], region: Region, radius: f32) {
        let Some(globe) = self.gpu.globe.as_mut() else {
            return;
        };
        let (released_bytes, allocated_bytes) =
            globe.install_detail(&self.gpu.device, &self.gpu.queue, pixels, region, radius);
        self.gpu.fixed_gpu_bytes = self
            .gpu
            .fixed_gpu_bytes
            .saturating_sub(released_bytes)
            .saturating_add(allocated_bytes);
    }

    pub(super) fn update_globe_detail(&mut self) {
        if !self.overview_open || !self.is_earth_world() || self.gpu.globe.is_none() {
            return;
        }
        if self.overview_zoom > 1.12 {
            self.globe_detail_loading = None;
            self.clear_globe_detail();
            return;
        }
        if let Some(receiver) = &self.globe_detail_loading {
            match receiver.try_recv() {
                Ok((region, pixels)) => {
                    if let Some(manifest) =
                        self.game.world_manifests.get(&self.game.current_instance)
                    {
                        let radius = minimium_procgen::planet_surface_for(*manifest).radius_mm()
                            as f32
                            / 1000.0;
                        self.install_globe_detail(&pixels, region, radius);
                    }
                    self.globe_detail_loading = None;
                }
                Err(TryRecvError::Disconnected) => self.globe_detail_loading = None,
                Err(TryRecvError::Empty) => return,
            }
        }
        let (view, _) = self.overview_camera();
        let direction = view
            .inverse()
            .transform_point3(glam::Vec3::ZERO)
            .normalize();
        let wanted = Region::around(direction, self.overview_zoom);
        if self
            .gpu
            .globe
            .as_ref()
            .and_then(|globe| globe.detail.as_ref())
            .is_some_and(|detail| detail.region.covers(wanted))
        {
            return;
        }
        let Some(planet) = self
            .game
            .world_manifests
            .get(&self.game.current_instance)
            .and_then(|manifest| manifest.planet())
        else {
            return;
        };
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let Some(geography) = self
            .connection
            .session()
            .and_then(|session| session.prepared_geography())
        else {
            return;
        };
        let scale = planet.height_scale_permille;
        let sea = planet.sea_level_mm;
        std::thread::spawn(move || {
            let pixels = super::globe::pixels_region(
                geography,
                scale,
                sea,
                minimium_renderer::globe_gpu::DETAIL_TILE_WIDTH,
                minimium_renderer::globe_gpu::DETAIL_TILE_HEIGHT,
                wanted,
            );
            let _ = sender.send((wanted, pixels));
        });
        self.globe_detail_loading = Some(receiver);
    }
}
