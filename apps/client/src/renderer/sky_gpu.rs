use super::geometry::Vertex;
use std::borrow::Cow;

pub(super) struct EnvironmentPipelines {
    pub(super) sky: wgpu::RenderPipeline,
    pub(super) sun: wgpu::RenderPipeline,
    pub(super) marker: wgpu::RenderPipeline,
}

struct PipelineConfig {
    format: wgpu::TextureFormat,
    label: &'static str,
    vertex_layout: Option<wgpu::VertexBufferLayout<'static>>,
    depth_compare: wgpu::CompareFunction,
    depth_write: bool,
    blend: Option<wgpu::BlendState>,
}

impl EnvironmentPipelines {
    pub(super) fn new(
        device: &wgpu::Device,
        frame_layout: &wgpu::BindGroupLayout,
        milky_way_layout: &wgpu::BindGroupLayout,
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
            Cow::Owned(super::shader_source::sky()),
            PipelineConfig {
                format,
                label: "clear-sky-pipeline",
                vertex_layout: None,
                depth_compare: wgpu::CompareFunction::Always,
                depth_write: false,
                blend: None,
            },
        );
        let sun = create_pipeline(
            device,
            &layout,
            Cow::Owned(super::shader_source::sun()),
            PipelineConfig {
                format,
                label: "sun-disk-pipeline",
                vertex_layout: None,
                depth_compare: wgpu::CompareFunction::Always,
                depth_write: false,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            },
        );
        let marker = create_pipeline(
            device,
            &layout,
            Cow::Owned(super::shader_source::marker()),
            PipelineConfig {
                format,
                label: "overview-marker-pipeline",
                vertex_layout: Some(Vertex::layout()),
                depth_compare: wgpu::CompareFunction::LessEqual,
                depth_write: true,
                blend: None,
            },
        );
        Self { sky, sun, marker }
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
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[config.vertex_layout],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: config.format,
                blend: config.blend,
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
