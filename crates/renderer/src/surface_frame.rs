//! Window-independent acquisition and presentation of a WGPU surface frame.

/// One acquired surface image and its view, ready for command encoding.
pub struct SurfaceFrame {
    pub view: wgpu::TextureView,
    texture: wgpu::SurfaceTexture,
}

/// Outcome of attempting to acquire the next surface image.
pub enum SurfaceAcquire {
    Ready(SurfaceFrame),
    Skipped,
    Retry(wgpu::CurrentSurfaceTexture),
}

/// Acquires a surface image and handles the recoverable suboptimal state.
pub fn acquire(
    surface: &wgpu::Surface<'_>,
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> SurfaceAcquire {
    let texture = match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(texture) => texture,
        wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
            surface.configure(device, config);
            texture
        }
        wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
            return SurfaceAcquire::Skipped;
        }
        error => return SurfaceAcquire::Retry(error),
    };
    let view = texture
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    SurfaceAcquire::Ready(SurfaceFrame { view, texture })
}

impl SurfaceFrame {
    /// Submits encoded commands and presents this surface image.
    pub fn submit(self, queue: &wgpu::Queue, encoder: wgpu::CommandEncoder) {
        queue.submit(Some(encoder.finish()));
        queue.present(self.texture);
    }
}
