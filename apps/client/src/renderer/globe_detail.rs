use super::{globe::GlobeVertex, GpuResources, Renderer};
use std::f64::consts::{PI, TAU};
use std::sync::mpsc::TryRecvError;
use wgpu::util::DeviceExt;

const TILE_WIDTH: u32 = 1024;
const TILE_HEIGHT: u32 = 512;
const TILES: u32 = 16;

#[derive(Clone, Copy)]
pub(super) struct Region {
    pub u_start: f64,
    pub v_start: f64,
    pub u_span: f64,
    pub v_span: f64,
}

impl Region {
    pub const FULL: Self = Self {
        u_start: 0.0,
        v_start: 0.0,
        u_span: 1.0,
        v_span: 1.0,
    };

    fn around(direction: glam::Vec3, zoom: f32) -> Self {
        let latitude = f64::from(direction.y).asin();
        let u = (f64::from(direction.z.atan2(direction.x)) / TAU + 0.5).rem_euclid(1.0);
        let v = 0.5 - latitude / PI;
        let horizon = (1.0 / f64::from(zoom)).acos();
        let v_span = (3.0 * horizon / PI).clamp(0.04, 0.5);
        let u_span = (3.0 * horizon / (TAU * latitude.cos().abs().max(0.25))).clamp(0.04, 0.5);
        Self {
            u_start: u - u_span * 0.5,
            v_start: (v - v_span * 0.5).clamp(0.0, 1.0 - v_span),
            u_span,
            v_span,
        }
    }

    fn covers(self, wanted: Self) -> bool {
        let u_shift = ((wanted.u_start - self.u_start + 0.5).rem_euclid(1.0) - 0.5).abs();
        let v_shift = (wanted.v_start - self.v_start).abs();
        u_shift < self.u_span * 0.15
            && v_shift < self.v_span * 0.15
            && (wanted.u_span / self.u_span - 1.0).abs() < 0.2
            && (wanted.v_span / self.v_span - 1.0).abs() < 0.2
    }
}

pub(super) struct DetailGpu {
    pub bind_group: wgpu::BindGroup,
    pub vertices: wgpu::Buffer,
    pub vertex_count: u32,
    bytes: u64,
    region: Region,
}

fn patch_vertices(radius: f32, region: Region) -> Vec<GlobeVertex> {
    const COLUMNS: u32 = 256;
    const ROWS: u32 = 128;
    let mut vertices = Vec::with_capacity((COLUMNS * ROWS * 6) as usize);
    let point = |x: u32, y: u32| {
        let u = f64::from(x) / f64::from(COLUMNS);
        let v = f64::from(y) / f64::from(ROWS);
        let lon = ((region.u_start + u * region.u_span) * 360.0 - 180.0).to_radians();
        let lat = (90.0 - (region.v_start + v * region.v_span) * 180.0).to_radians();
        let r = f64::from(radius) * 1.00001;
        GlobeVertex {
            position: [
                (r * lat.cos() * lon.cos()) as f32,
                (r * lat.sin()) as f32,
                (r * lat.cos() * lon.sin()) as f32,
            ],
            uv: [u as f32, v as f32],
        }
    };
    for y in 0..ROWS {
        for x in 0..COLUMNS {
            let (a, b, c, d) = (
                point(x, y),
                point(x + 1, y),
                point(x, y + 1),
                point(x + 1, y + 1),
            );
            vertices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    vertices
}

impl GpuResources {
    fn clear_globe_detail(&mut self) {
        if let Some(detail) = self.globe.as_mut().and_then(|globe| globe.detail.take()) {
            self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_sub(detail.bytes);
        }
    }

    fn install_globe_detail(&mut self, pixels: &[u8], region: Region, radius: f32) {
        let Some(globe) = self.globe.as_mut() else {
            return;
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GEBCO local detail"),
            size: wgpu::Extent3d {
                width: TILE_WIDTH,
                height: TILE_HEIGHT,
                depth_or_array_layers: TILES,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TILE_WIDTH * 4),
                rows_per_image: Some(TILE_HEIGHT),
            },
            wgpu::Extent3d {
                width: TILE_WIDTH,
                height: TILE_HEIGHT,
                depth_or_array_layers: TILES,
            },
        );
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globe local detail bind group"),
            layout: &globe.pipeline.get_bind_group_layout(1),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.create_view(
                        &wgpu::TextureViewDescriptor {
                            dimension: Some(wgpu::TextureViewDimension::D2Array),
                            ..Default::default()
                        },
                    )),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let mesh = patch_vertices(radius, region);
        let vertex_count = mesh.len() as u32;
        let vertices = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("globe local detail mesh"),
                contents: bytemuck::cast_slice(&mesh),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let bytes = pixels.len() as u64 + vertices.size();
        if let Some(previous) = globe.detail.take() {
            self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_sub(previous.bytes);
        }
        self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_add(bytes);
        globe.detail = Some(DetailGpu {
            bind_group,
            vertices,
            vertex_count,
            bytes,
            region,
        });
    }
}

impl Renderer {
    pub(super) fn update_globe_detail(&mut self) {
        if !self.overview_open || !self.is_earth_world() || self.gpu.globe.is_none() {
            return;
        }
        if self.overview_zoom > 1.12 {
            self.globe_detail_loading = None;
            self.gpu.clear_globe_detail();
            return;
        }
        if let Some(receiver) = &self.globe_detail_loading {
            match receiver.try_recv() {
                Ok((region, pixels)) => {
                    if let Some(manifest) = self.world_manifests.get(&self.current_instance) {
                        let radius = minimium_procgen::planet_surface_for(*manifest).radius_mm()
                            as f32
                            / 1000.0;
                        self.gpu.install_globe_detail(&pixels, region, radius);
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
            .world_manifests
            .get(&self.current_instance)
            .and_then(|manifest| manifest.planet())
        else {
            return;
        };
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let scale = planet.height_scale_permille;
        let sea = planet.sea_level_mm;
        std::thread::spawn(move || {
            let pixels = super::globe::pixels_region(scale, sea, TILE_WIDTH, TILE_HEIGHT, wanted);
            let _ = sender.send((wanted, pixels));
        });
        self.globe_detail_loading = Some(receiver);
    }
}
