use super::{Atlas, CloudInstances};
use crate::environment::weather::{clouds::runtime::PresentedWeather, ATLAS_LAYERS};
use std::sync::Arc;

impl CloudInstances {
    pub(in crate::renderer) fn set_atlas(
        &mut self,
        pixels: [&[u8]; ATLAS_LAYERS],
        revision: u64,
        region: [f32; 4],
        start_seconds: Option<f64>,
    ) {
        self.atlas = Some(Arc::new(Atlas {
            pixels: pixels.map(<[u8]>::to_vec),
            revision,
            region,
            start_seconds,
        }));
    }

    pub(in crate::renderer) fn presentation(
        &self,
        location: (f64, f64),
        _utc_ms: i64,
    ) -> Option<PresentedWeather> {
        Some(self.frame.as_ref()?.presentation(location, self.morph))
    }

    pub(in crate::renderer) fn publish_atlas(
        &self,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        revision: &mut u64,
    ) {
        let atlas = self.published.as_ref().map(|request| &request.atlas);
        let next = atlas.map_or(u64::MAX, |atlas| atlas.revision);
        if *revision == next {
            return;
        }
        let empty = atlas.is_none().then(|| vec![0; 512 * 256 * 4]);
        for layer in 0..ATLAS_LAYERS {
            let pixels = atlas.map_or_else(
                || empty.as_ref().unwrap().as_slice(),
                |atlas| atlas.pixels[layer].as_slice(),
            );
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(512 * 4),
                    rows_per_image: Some(256),
                },
                wgpu::Extent3d {
                    width: 512,
                    height: 256,
                    depth_or_array_layers: 1,
                },
            );
        }
        *revision = next;
    }
}
