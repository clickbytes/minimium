use crate::ShadowSettings;

pub const DEFAULT_RESOLUTION: u32 = 1024;

pub fn supported_format(
    adapter: &wgpu::Adapter,
    usages: wgpu::TextureUsages,
) -> Option<wgpu::TextureFormat> {
    [
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureFormat::Depth24Plus,
        wgpu::TextureFormat::Depth16Unorm,
    ]
    .into_iter()
    .find(|format| {
        adapter
            .get_texture_format_features(*format)
            .allowed_usages
            .contains(usages)
    })
}

pub(crate) struct ShadowTargets {
    pub texture: wgpu::Texture,
    pub local_view: wgpu::TextureView,
    pub landscape_view: wgpu::TextureView,
    pub moon_views: [wgpu::TextureView; 2],
    pub sampling: wgpu::BindGroup,
    pub bytes: u64,
}

pub(crate) fn create_target_checked(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    landscape_uniform: &wgpu::Buffer,
    moon_uniforms: &[wgpu::Buffer; 2],
    sampler: &wgpu::Sampler,
    settings: ShadowSettings,
    format: wgpu::TextureFormat,
) -> Result<ShadowTargets, &'static str> {
    let out_of_memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let target = create_target(
        device,
        layout,
        uniform,
        landscape_uniform,
        moon_uniforms,
        sampler,
        settings,
        format,
    );
    let validation_error = pollster::block_on(validation.pop());
    let out_of_memory_error = pollster::block_on(out_of_memory.pop());
    if validation_error.is_some() {
        drop(target);
        Err("shadow_resource_validation_failed")
    } else if out_of_memory_error.is_some() {
        drop(target);
        Err("shadow_resource_allocation_failed")
    } else {
        Ok(target)
    }
}

fn create_target(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    landscape_uniform: &wgpu::Buffer,
    moon_uniforms: &[wgpu::Buffer; 2],
    sampler: &wgpu::Sampler,
    settings: ShadowSettings,
    format: wgpu::TextureFormat,
) -> ShadowTargets {
    let resolution = if settings.enabled {
        settings.resolution.clamp(1, DEFAULT_RESOLUTION)
    } else {
        1
    };
    let layers = if settings.enabled { 4 } else { 1 };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("surface-shadow-depth-maps"),
        size: wgpu::Extent3d {
            width: resolution,
            height: resolution,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let layer_view = |layer| {
        texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..Default::default()
        })
    };
    let local_view = layer_view(0);
    let landscape_view = layer_view(1.min(layers - 1));
    let moon_views = [2, 3].map(|i| layer_view(i.min(layers - 1)));
    let sampling_view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let sampling_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("world-shadow-sampling-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&sampling_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: moon_uniforms[0].as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: moon_uniforms[1].as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: landscape_uniform.as_entire_binding(),
            },
        ],
    });
    let bytes_per_pixel = if format == wgpu::TextureFormat::Depth16Unorm {
        2
    } else {
        4
    };
    let bytes = u64::from(resolution).pow(2) * u64::from(layers) * bytes_per_pixel;
    ShadowTargets {
        texture,
        local_view,
        landscape_view,
        moon_views,
        sampling: sampling_bind_group,
        bytes,
    }
}

pub(crate) fn sampling_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("world-shadow-sampling-layout"),
        entries: &[
            depth_layout_entry(0),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
            uniform_layout_entry(
                2,
                wgpu::ShaderStages::VERTEX_FRAGMENT | wgpu::ShaderStages::COMPUTE,
            ),
            uniform_layout_entry(
                5,
                wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
            ),
            uniform_layout_entry(
                6,
                wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
            ),
            uniform_layout_entry(
                4,
                wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
            ),
        ],
    })
}

pub(crate) fn depth_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}

pub fn uniform_layout_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(128),
        },
        count: None,
    }
}
