//! Camera-independent optical depth table; rebuilt only with the GPU resources.
//! Metres and coefficients here match atmosphere_common.wgsl, regardless of game scale.
use crate::atmosphere::Optics;
use glam::DVec3;

pub(super) const WIDTH: u32 = 128;
pub(super) const HEIGHT: u32 = 64;
pub const BYTES: u64 = WIDTH as u64 * HEIGHT as u64 * 8;

pub fn entry() -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: 1,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

pub struct OpticalTables {
    pub depth: wgpu::TextureView,
    pub irradiance: wgpu::TextureView,
}

impl OpticalTables {
    pub const BYTES: u64 =
        BYTES + crate::sky_irradiance::WIDTH as u64 * crate::sky_irradiance::HEIGHT as u64 * 8;

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, profile: Optics) -> Self {
        let tables = Self {
            depth: texture(device, "spherical atmosphere optical depth", WIDTH, HEIGHT),
            irradiance: texture(
                device,
                "upper hemisphere unit irradiance",
                crate::sky_irradiance::WIDTH,
                crate::sky_irradiance::HEIGHT,
            ),
        };
        tables.update(queue, profile);
        tables
    }

    pub fn update(&self, queue: &wgpu::Queue, profile: Optics) {
        let depth = pixels(profile);
        let irradiance = crate::sky_irradiance::pixels(profile, &depth);
        upload(queue, self.depth.texture(), &depth);
        upload(queue, self.irradiance.texture(), &irradiance);
    }
}

fn texture(device: &wgpu::Device, label: &str, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

// Standalone optical-depth consumers can still allocate just the original table.
pub fn create(device: &wgpu::Device, queue: &wgpu::Queue, profile: Optics) -> wgpu::TextureView {
    let view = texture(device, "spherical atmosphere optical depth", WIDTH, HEIGHT);
    update(queue, view.texture(), profile);
    view
}

fn pixels(profile: Optics) -> Vec<u16> {
    let radius = profile.radius_m;
    let top = profile.atmospheric_top_m;
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        let altitude = (f64::from(y) / f64::from(HEIGHT - 1)).powi(2) * top;
        let r = radius + altitude;
        let horizon = -(1.0 - (radius / r).powi(2)).max(0.0).sqrt();
        for x in 0..WIDTH {
            // Allocate most samples near the geometric horizon, including twilight aloft.
            let u = f64::from(x) / f64::from(WIDTH - 1);
            let mu = horizon + (1.0 - horizon) * u * u;
            let length = -r * mu + ((r * mu).powi(2) + (radius + top).powi(2) - r * r).sqrt();
            let mut optical = DVec3::ZERO;
            for step in 0..64 {
                let d = length * (f64::from(step) + 0.5) / 64.0;
                let h = (r * r + d * d + 2.0 * r * mu * d).sqrt() - radius;
                let rayleigh = (-h / profile.rayleigh_scale_height_m).exp();
                let mie = (-h / profile.aerosol_scale_height_m).exp();
                let ozone = (1.0
                    - (h - profile.ozone_peak_height_m).abs() / profile.ozone_half_width_m)
                    .max(0.0);
                optical += (DVec3::from_array(profile.rayleigh_rgb_m_inv) * rayleigh
                    + DVec3::splat(profile.mie_m_inv) * mie
                    + DVec3::from_array(profile.ozone_rgb_m_inv) * ozone)
                    * (length / 64.0);
            }
            pixels.extend(optical.to_array().map(|v| half::f16::from_f64(v).to_bits()));
            pixels.push(0);
        }
    }
    pixels
}

pub fn update(queue: &wgpu::Queue, texture: &wgpu::Texture, profile: Optics) {
    upload(queue, texture, &pixels(profile));
}

fn upload(queue: &wgpu::Queue, texture: &wgpu::Texture, data: &[u16]) {
    queue.write_texture(
        texture.as_image_copy(),
        bytemuck::cast_slice(data),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(texture.width() * 8),
            rows_per_image: Some(texture.height()),
        },
        texture.size(),
    );
}
