use super::GpuResources;
use std::borrow::Cow;
use wgpu::util::DeviceExt;

const MAX_TILE_WIDTH: u32 = 2048;
const TILES_PER_AXIS: u32 = 4;
const TILE_COUNT: u32 = TILES_PER_AXIS * TILES_PER_AXIS;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct GlobeVertex {
    pub(super) position: [f32; 3],
    pub(super) uv: [f32; 2],
}

pub(super) struct GlobeGpu {
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) vertices: wgpu::Buffer,
    pub(super) vertex_count: u32,
    lod: u8,
    pub(super) detail: Option<super::globe_detail::DetailGpu>,
}

/// Samples the pinned package on a worker thread, away from redraw handling.
pub(super) fn pixels(
    height_scale_permille: u32,
    sea_level_mm: i64,
    width: u32,
    height: u32,
) -> Vec<u8> {
    pixels_region(
        height_scale_permille,
        sea_level_mm,
        width,
        height,
        super::globe_detail::Region::FULL,
    )
}

pub(super) fn pixels_region(
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
            let elevation = minimium_procgen::earth_elevation_mm(direction, height_scale_permille);
            let metres = ((elevation - sea_level_mm) / 1000) as f32;
            let lake = crate::environment::weather::lakes::contains(
                latitude.to_degrees(),
                longitude.to_degrees(),
            );
            let color: [u8; 3] = if lake {
                [22, 72, 112]
            } else if metres < 0.0 {
                let deep = (-metres / 400.0).clamp(0.0, 1.0);
                [
                    ((20.0 - 12.0 * deep) as u8),
                    ((82.0 - 39.0 * deep) as u8),
                    ((145.0 - 44.0 * deep) as u8),
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
                    ((133.0 + 100.0 * snow) as u8),
                    ((125.0 + 105.0 * snow) as u8),
                    ((114.0 + 120.0 * snow) as u8),
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

fn mesh_resolution(zoom: f32) -> (u32, u32, u8) {
    if zoom <= 1.05 {
        (1280, 640, 7)
    } else if zoom <= 1.10 {
        (1152, 576, 6)
    } else if zoom <= 1.20 {
        (1024, 512, 5)
    } else if zoom <= 1.50 {
        (896, 448, 4)
    } else if zoom <= 2.0 {
        (768, 384, 3)
    } else if zoom <= 3.0 {
        (640, 320, 2)
    } else if zoom <= 5.0 {
        (512, 256, 1)
    } else {
        (256, 128, 0)
    }
}

fn vertices(radius: f32, zoom: f32) -> Vec<GlobeVertex> {
    let (longitudes, latitudes, _) = mesh_resolution(zoom);
    let mut out = Vec::with_capacity((longitudes * latitudes * 6) as usize);
    let point = |x: u32, y: u32| {
        let u = x as f32 / longitudes as f32;
        let v = y as f32 / latitudes as f32;
        let lon = (u * 360.0 - 180.0).to_radians();
        let lat = (90.0 - v * 180.0).to_radians();
        GlobeVertex {
            position: [
                radius * lat.cos() * lon.cos(),
                radius * lat.sin(),
                radius * lat.cos() * lon.sin(),
            ],
            uv: [u, v],
        }
    };
    for y in 0..latitudes {
        for x in 0..longitudes {
            let (a, b, c, d) = (
                point(x, y),
                point(x + 1, y),
                point(x, y + 1),
                point(x + 1, y + 1),
            );
            out.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    out
}

impl GpuResources {
    pub(super) fn install_globe(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
        radius: f32,
        zoom: f32,
    ) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GEBCO overview"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: TILE_COUNT,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: TILE_COUNT,
            },
        );
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("globe texture layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globe texture bind group"),
            layout: &layout,
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
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("globe shader"),
                source: wgpu::ShaderSource::Wgsl(Cow::Owned(super::shader_source::globe())),
            });
        let camera_layout = self.pipeline.get_bind_group_layout(0);
        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("globe pipeline layout"),
                bind_group_layouts: &[
                    Some(&camera_layout),
                    Some(&layout),
                    Some(&self.weather_gpu.globe_layout),
                ],
                immediate_size: 0,
            });
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("globe pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<GlobeVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.scene_color_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
        let mesh = vertices(radius, zoom);
        let (_, _, lod) = mesh_resolution(zoom);
        let vertex_count = mesh.len() as u32;
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("globe sphere"),
                contents: bytemuck::cast_slice(&mesh),
                usage: wgpu::BufferUsages::VERTEX,
            });
        self.fixed_gpu_bytes += vertex_buffer.size() + pixels.len() as u64;
        self.globe = Some(GlobeGpu {
            pipeline,
            bind_group,
            vertices: vertex_buffer,
            vertex_count,
            lod,
            detail: None,
        });
    }

    pub(super) fn texture_dimensions(&self) -> (u32, u32) {
        let width = self
            .device
            .limits()
            .max_texture_dimension_2d
            .min(MAX_TILE_WIDTH);
        (width, width / 2)
    }

    pub(super) fn refresh_globe_lod(&mut self, radius: f32, zoom: f32) {
        let (_, _, lod) = mesh_resolution(zoom);
        let Some(globe) = self.globe.as_mut() else {
            return;
        };
        if globe.lod == lod {
            return;
        }
        let mesh = vertices(radius, zoom);
        let vertex_count = mesh.len() as u32;
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("globe sphere LOD"),
                contents: bytemuck::cast_slice(&mesh),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let Some(globe) = self.globe.as_mut() else {
            return;
        };
        self.fixed_gpu_bytes = self
            .fixed_gpu_bytes
            .saturating_sub(globe.vertices.size())
            .saturating_add(buffer.size());
        globe.vertices = buffer;
        globe.vertex_count = vertex_count;
        globe.lod = lod;
    }
}
