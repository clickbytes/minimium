//! Texture lifetime/upload for static-coordinate weather grids; no climate rules.
use crate::{surface_atlas, surface_cover};
use std::sync::Arc;

pub struct TerrainCoverGpu {
    pub local: wgpu::Texture,
    pub global: wgpu::Texture,
    pub start_utc: Option<f64>,
    atlas: Option<Arc<surface_atlas::SurfaceAtlas>>,
}

impl TerrainCoverGpu {
    pub const BYTES: u64 =
        surface_cover::EDGE as u64 * surface_cover::EDGE as u64 * 32 + surface_atlas::BYTES;

    pub fn new(device: &wgpu::Device) -> Self {
        let make = |label, width, height| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 4,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        Self {
            local: make(
                "local surface history",
                surface_cover::EDGE,
                surface_cover::EDGE,
            ),
            global: make(
                "planet surface history",
                surface_atlas::WIDTH,
                surface_atlas::HEIGHT,
            ),
            start_utc: None,
            atlas: None,
        }
    }

    pub fn clear(&mut self) {
        self.start_utc = None;
        self.atlas = None;
    }

    pub fn upload(&mut self, queue: &wgpu::Queue, grid: &surface_cover::CoverGrid) -> bool {
        self.start_utc = Some(grid.start_utc);
        write(queue, &self.local, &grid.pixels);
        if let Some(atlas) = grid
            .atlas
            .as_ref()
            .filter(|a| a.start_utc == grid.start_utc)
        {
            if self
                .atlas
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, atlas))
            {
                write(queue, &self.global, &atlas.pixels);
                self.atlas = Some(atlas.clone());
            }
            true
        } else {
            self.atlas = None;
            false
        }
    }
}

fn write(queue: &wgpu::Queue, texture: &wgpu::Texture, pixels: &[[half::f16; 4]]) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(pixels),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(texture.width() * 8),
            rows_per_image: Some(texture.height()),
        },
        texture.size(),
    );
}
