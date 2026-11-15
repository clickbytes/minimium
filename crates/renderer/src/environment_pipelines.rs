use crate::geometry::Vertex;
use std::borrow::Cow;

pub struct EnvironmentPipelines {
    pub sky: wgpu::RenderPipeline,
    pub bolid_dust: wgpu::RenderPipeline,
    pub bolid_emission: wgpu::RenderPipeline,
    pub bolid_composite: wgpu::RenderPipeline,
    pub sun: wgpu::RenderPipeline,
    pub marker: wgpu::RenderPipeline,
}

struct PipelineConfig {
    format: wgpu::TextureFormat,
    label: &'static str,
    vertex_layout: Option<wgpu::VertexBufferLayout<'static>>,
    depth_compare: wgpu::CompareFunction,
    depth_write: bool,
    blend: Option<wgpu::BlendState>,
    fragment: &'static str,
    scene_depth: bool,
}

impl EnvironmentPipelines {
    pub fn new(
        device: &wgpu::Device,
        frame_layout: &wgpu::BindGroupLayout,
        milky_way_layout: &wgpu::BindGroupLayout,
        bolid_depth_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("environment-frame-pipeline-layout"),
            bind_group_layouts: &[Some(frame_layout)],
            immediate_size: 0,
        });
        let sky_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky-with-milky-way-pipeline-layout"),
            bind_group_layouts: &[Some(frame_layout), Some(milky_way_layout)],
            immediate_size: 0,
        });
        let sky = create_pipeline(
            device,
            &sky_layout,
            Cow::Owned(crate::shader_source::sky()),
            PipelineConfig {
                format,
                label: "clear-sky-pipeline",
                fragment: "fs_main",
                scene_depth: true,
                vertex_layout: None,
                depth_compare: wgpu::CompareFunction::Always,
                depth_write: false,
                blend: None,
            },
        );
        let bolid_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bolid finite-distance layout"),
            bind_group_layouts: &[
                Some(frame_layout),
                Some(milky_way_layout),
                Some(bolid_depth_layout),
            ],
            immediate_size: 0,
        });
        let bolid_dust = create_pipeline(
            device,
            &bolid_layout,
            Cow::Owned(crate::shader_source::sky()),
            PipelineConfig {
                format,
                label: "bolid-dust-pipeline",
                fragment: "fs_dust",
                scene_depth: false,
                vertex_layout: None,
                depth_compare: wgpu::CompareFunction::LessEqual,
                depth_write: false,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            },
        );
        let additive = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let bolid_emission = create_pipeline(
            device,
            &bolid_layout,
            Cow::Owned(crate::shader_source::sky()),
            PipelineConfig {
                format,
                label: "bolid-emission-pipeline",
                fragment: "fs_bolid",
                scene_depth: false,
                vertex_layout: None,
                depth_compare: wgpu::CompareFunction::Always,
                depth_write: false,
                blend: Some(wgpu::BlendState {
                    color: additive,
                    alpha: wgpu::BlendComponent::OVER,
                }),
            },
        );
        let sun = create_pipeline(
            device,
            &layout,
            Cow::Owned(crate::shader_source::sun()),
            PipelineConfig {
                format,
                label: "sun-disk-pipeline",
                scene_depth: true,
                fragment: "fs_main",
                vertex_layout: None,
                depth_compare: wgpu::CompareFunction::Always,
                depth_write: false,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            },
        );
        let marker = create_pipeline(
            device,
            &layout,
            Cow::Owned(crate::shader_source::marker()),
            PipelineConfig {
                format,
                label: "overview-marker-pipeline",
                scene_depth: true,
                fragment: "fs_main",
                vertex_layout: Some(Vertex::layout()),
                depth_compare: wgpu::CompareFunction::LessEqual,
                depth_write: true,
                blend: None,
            },
        );
        Self {
            sky,
            bolid_dust,
            bolid_emission,
            bolid_composite: crate::bolid_layer::composite_pipeline(device, format),
            sun,
            marker,
        }
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    source: Cow<'static, str>,
    config: PipelineConfig,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(config.label),
        source: wgpu::ShaderSource::Wgsl(source),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(config.label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some(match config.fragment {
                "fs_bolid" => "vs_bolid",
                "fs_dust" => "vs_bolid_dust",
                _ => "vs_main",
            }),
            compilation_options: Default::default(),
            buffers: &[config.vertex_layout],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(config.fragment),
            compilation_options: Default::default(),
            targets: &if config.scene_depth {
                vec![Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: config.blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })]
            } else {
                let add = wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                };
                vec![
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: config.blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: Some(wgpu::BlendState {
                            color: add,
                            alpha: add,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ]
            },
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: config.scene_depth.then_some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(config.depth_write),
            depth_compare: Some(config.depth_compare),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}
