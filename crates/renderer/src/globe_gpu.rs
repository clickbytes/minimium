//! Overview globe texture, pipeline and LOD mesh resources.
use crate::{
    globe_detail::{patch_vertices, DetailGpu, Region},
    GlobeVertex,
};
use std::borrow::Cow;
use wgpu::util::DeviceExt;

const MAX_TILE_WIDTH: u32 = 2048;
const TILES_PER_AXIS: u32 = 4;
const TILE_COUNT: u32 = TILES_PER_AXIS * TILES_PER_AXIS;
pub const DETAIL_TILE_WIDTH: u32 = 1024;
pub const DETAIL_TILE_HEIGHT: u32 = 512;

pub struct GlobeGpu {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub vertices: wgpu::Buffer,
    pub vertex_count: u32,
    pub detail: Option<DetailGpu>,
    lod: u8,
}

impl GlobeGpu {
    pub fn clear_detail(&mut self) -> u64 {
        self.detail.take().map_or(0, |detail| detail.bytes)
    }

    pub fn install_detail(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixels: &[u8],
        region: Region,
        radius: f32,
    ) -> (u64, u64) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GEBCO local detail"),
            size: wgpu::Extent3d {
                width: DETAIL_TILE_WIDTH,
                height: DETAIL_TILE_HEIGHT,
                depth_or_array_layers: TILE_COUNT,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(DETAIL_TILE_WIDTH * 4),
                rows_per_image: Some(DETAIL_TILE_HEIGHT),
            },
            wgpu::Extent3d {
                width: DETAIL_TILE_WIDTH,
                height: DETAIL_TILE_HEIGHT,
                depth_or_array_layers: TILE_COUNT,
            },
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globe local detail bind group"),
            layout: &self.pipeline.get_bind_group_layout(1),
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
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("globe local detail mesh"),
            contents: bytemuck::cast_slice(&mesh),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let bytes = pixels.len() as u64 + vertices.size();
        let released_bytes = self.clear_detail();
        self.detail = Some(DetailGpu {
            bind_group,
            vertices,
            vertex_count,
            bytes,
            region,
        });
        (released_bytes, bytes)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixels: &[u8],
        width: u32,
        height: u32,
        radius: f32,
        zoom: f32,
        camera_layout: &wgpu::BindGroupLayout,
        atmosphere_layout: &wgpu::BindGroupLayout,
        color_format: wgpu::TextureFormat,
    ) -> (Self, u64) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
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
        queue.write_texture(
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globe texture bind group"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("globe shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(crate::shader_source::globe())),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("globe pipeline layout"),
            bind_group_layouts: &[
                Some(camera_layout),
                Some(&texture_layout),
                Some(atmosphere_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("globe pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(GlobeVertex::layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
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
        let (vertex_buffer, vertex_count, lod) = create_mesh(device, radius, zoom, "globe sphere");
        let bytes = vertex_buffer.size().saturating_add(pixels.len() as u64);
        (
            Self {
                pipeline,
                bind_group,
                vertices: vertex_buffer,
                vertex_count,
                detail: None,
                lod,
            },
            bytes,
        )
    }

    pub fn texture_dimensions(device: &wgpu::Device) -> (u32, u32) {
        let width = device.limits().max_texture_dimension_2d.min(MAX_TILE_WIDTH);
        (width, width / 2)
    }

    pub fn refresh_lod(&mut self, device: &wgpu::Device, radius: f32, zoom: f32) -> (u64, u64) {
        let (_, _, lod) = mesh_resolution(zoom);
        if self.lod == lod {
            return (0, 0);
        }
        let (buffer, count, _) = create_mesh(device, radius, zoom, "globe sphere LOD");
        let old_size = self.vertices.size();
        let new_size = buffer.size();
        self.vertices = buffer;
        self.vertex_count = count;
        self.lod = lod;
        (old_size, new_size)
    }
}

fn create_mesh(
    device: &wgpu::Device,
    radius: f32,
    zoom: f32,
    label: &str,
) -> (wgpu::Buffer, u32, u8) {
    let (longitudes, latitudes, lod) = mesh_resolution(zoom);
    let mut vertices = Vec::with_capacity((longitudes * latitudes * 6) as usize);
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
            vertices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    let count = vertices.len() as u32;
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    (buffer, count, lod)
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
