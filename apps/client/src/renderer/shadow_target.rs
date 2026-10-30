use super::shadows::ShadowSettings;

pub(super) const DEFAULT_RESOLUTION: u32 = 1024;

pub(super) fn supported_format(
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

pub(super) fn create_target_checked(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    settings: ShadowSettings,
    format: wgpu::TextureFormat,
) -> Result<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup, u64), &'static str> {
    let out_of_memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let target = create_target(device, layout, uniform, sampler, settings, format);
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
    sampler: &wgpu::Sampler,
    settings: ShadowSettings,
    format: wgpu::TextureFormat,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::BindGroup, u64) {
    let resolution = if settings.enabled {
        settings.resolution.clamp(1, DEFAULT_RESOLUTION)
    } else {
        1
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("local-shadow-depth-map"),
        size: wgpu::Extent3d {
            width: resolution,
            height: resolution,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let depth_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampling_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("world-shadow-sampling-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&depth_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uniform.as_entire_binding(),
            },
        ],
    });
    let bytes_per_pixel = if format == wgpu::TextureFormat::Depth16Unorm {
        2
    } else {
        4
    };
    let bytes = u64::from(resolution) * u64::from(resolution) * bytes_per_pixel;
    (texture, depth_view, sampling_bind_group, bytes)
}

pub(super) fn uniform_layout_entry(
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
