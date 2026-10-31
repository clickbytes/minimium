//! Camera-independent optical depth table; rebuilt only with the GPU resources.
//! Metres and coefficients here match atmosphere_common.wgsl, regardless of game scale.
use glam::DVec3;

const WIDTH: u32 = 128;
const HEIGHT: u32 = 64;
pub(super) const BYTES: u64 = WIDTH as u64 * HEIGHT as u64 * 8;
const RADIUS: f64 = 6_371_000.0;
const TOP: f64 = 100_000.0;

pub(super) fn entry() -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: 4,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

pub(super) fn create(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        let altitude = (f64::from(y) / f64::from(HEIGHT - 1)).powi(2) * TOP;
        let r = RADIUS + altitude;
        let horizon = -(1.0 - (RADIUS / r).powi(2)).max(0.0).sqrt();
        for x in 0..WIDTH {
            // Allocate most samples near the geometric horizon, including twilight aloft.
            let u = f64::from(x) / f64::from(WIDTH - 1);
            let mu = horizon + (1.0 - horizon) * u * u;
            let length = -r * mu + ((r * mu).powi(2) + (RADIUS + TOP).powi(2) - r * r).sqrt();
            let mut optical = DVec3::ZERO;
            for step in 0..64 {
                let d = length * (f64::from(step) + 0.5) / 64.0;
                let h = (r * r + d * d + 2.0 * r * mu * d).sqrt() - RADIUS;
                let rayleigh = (-h / 8000.0).exp();
                let mie = (-h / 1200.0).exp();
                let ozone = (1.0 - (h - 25000.0).abs() / 15000.0).max(0.0);
                optical += (DVec3::new(5.802e-6, 13.558e-6, 33.100e-6) * rayleigh
                    + DVec3::splat(4.44e-6) * mie
                    + DVec3::new(0.650e-6, 1.881e-6, 0.085e-6) * ozone)
                    * (length / 64.0);
            }
            pixels.extend(optical.to_array().map(|v| half::f16::from_f64(v).to_bits()));
            pixels.push(0);
        }
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("spherical atmosphere optical depth"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        bytemuck::cast_slice(&pixels),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(WIDTH * 8),
            rows_per_image: Some(HEIGHT),
        },
        texture.size(),
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}
