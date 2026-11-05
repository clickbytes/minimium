pub(super) use minimium_renderer::GlobeGpu;

const TILES_PER_AXIS: u32 = 4;
const TILE_COUNT: u32 = TILES_PER_AXIS * TILES_PER_AXIS;

/// Samples the pinned package on a worker thread, away from redraw handling.
pub(super) fn pixels(
    geography: std::sync::Arc<dyn minimium_procgen::PreparedGeography>,
    height_scale_permille: u32,
    sea_level_mm: i64,
    width: u32,
    height: u32,
) -> Vec<u8> {
    pixels_region(
        geography,
        height_scale_permille,
        sea_level_mm,
        width,
        height,
        super::globe_detail::Region::FULL,
    )
}

pub(super) fn pixels_region(
    geography: std::sync::Arc<dyn minimium_procgen::PreparedGeography>,
    height_scale_permille: u32,
    sea_level_mm: i64,
    width: u32,
    height: u32,
    region: super::globe_detail::Region,
) -> Vec<u8> {
    let map_width = width * TILES_PER_AXIS;
    let map_height = height * TILES_PER_AXIS;
    let tile_bytes = (width * height * 4) as usize;
    let mut pixels = vec![0; tile_bytes * TILE_COUNT as usize];
    for y in 0..map_height {
        let v = region.v_start + (f64::from(y) + 0.5) / f64::from(map_height) * region.v_span;
        let latitude = (90.0 - v * 180.0).to_radians();
        for x in 0..map_width {
            let u = region.u_start + (f64::from(x) + 0.5) / f64::from(map_width) * region.u_span;
            let longitude = (u * 360.0 - 180.0).to_radians();
            let direction = [
                latitude.cos() * longitude.cos(),
                latitude.sin(),
                latitude.cos() * longitude.sin(),
            ];
            let elevation = geography
                .elevation_m(direction)
                .expect("prepared Earth geography must accept normalized globe directions");
            let elevation = (elevation * f64::from(height_scale_permille)).round() as i64;
            let metres = ((elevation - sea_level_mm) / 1000) as f32;
            let lake =
                crate::environment::lakes::contains(latitude.to_degrees(), longitude.to_degrees());
            let color: [u8; 3] = if lake {
                [22, 72, 112]
            } else if metres < 0.0 {
                let deep = (-metres / 400.0).clamp(0.0, 1.0);
                [
                    (20.0 - 12.0 * deep) as u8,
                    (82.0 - 39.0 * deep) as u8,
                    (145.0 - 44.0 * deep) as u8,
                ]
            } else if metres < 110.0 {
                [63, 122, 66]
            } else if metres < 280.0 {
                [84, 132, 70]
            } else if metres < 500.0 {
                [119, 126, 81]
            } else {
                let snow = ((metres - 500.0) / 400.0).clamp(0.0, 1.0);
                [
                    (133.0 + 100.0 * snow) as u8,
                    (125.0 + 105.0 * snow) as u8,
                    (114.0 + 120.0 * snow) as u8,
                ]
            };
            let layer = (y / height) * TILES_PER_AXIS + x / width;
            let local = (y % height) * width + x % width;
            let offset = layer as usize * tile_bytes + local as usize * 4;
            pixels[offset..offset + 3].copy_from_slice(&color);
            pixels[offset + 3] = ((metres + 500.0) / 1000.0 * 255.0).clamp(0.0, 255.0) as u8;
        }
    }
    pixels
}

impl super::GpuResources {
    pub(super) fn install_globe(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
        radius: f32,
        zoom: f32,
    ) {
        let (globe, bytes) = GlobeGpu::new(
            &self.device,
            &self.queue,
            pixels,
            width,
            height,
            radius,
            zoom,
            &self.pipeline.get_bind_group_layout(0),
            &self.atmosphere_gpu.layout,
            self.scene_color_format,
        );
        self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_add(bytes);
        self.globe = Some(globe);
    }

    pub(super) fn texture_dimensions(&self) -> (u32, u32) {
        GlobeGpu::texture_dimensions(&self.device)
    }

    pub(super) fn refresh_globe_lod(&mut self, radius: f32, zoom: f32) {
        if let Some(globe) = self.globe.as_mut() {
            let (old_size, new_size) = globe.refresh_lod(&self.device, radius, zoom);
            self.fixed_gpu_bytes = self
                .fixed_gpu_bytes
                .saturating_sub(old_size)
                .saturating_add(new_size);
        }
    }
}
