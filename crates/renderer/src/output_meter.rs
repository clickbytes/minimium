//! GPU-only frame luminance measurement and eye adaptation.
pub(super) struct FrameMeter {
    pipeline: wgpu::ComputePipeline,
    group: wgpu::BindGroup,
}

impl FrameMeter {
    pub(super) fn new(
        device: &wgpu::Device,
        frame_layout: &wgpu::BindGroupLayout,
        shadow_layout: &wgpu::BindGroupLayout,
        view: &wgpu::TextureView,
        params: &wgpu::Buffer,
        eye: &wgpu::Buffer,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("HDR luminance meter"),
            source: wgpu::ShaderSource::Wgsl(crate::shader_source::exposure_meter().into()),
        });
        let meter_layout = super::output::output_layout(
            device,
            wgpu::ShaderStages::COMPUTE,
            wgpu::BufferBindingType::Storage { read_only: false },
        );
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky and image exposure layout"),
            bind_group_layouts: &[Some(frame_layout), Some(&meter_layout), Some(shadow_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("HDR eye adaptation"),
            layout: Some(&layout),
            module: &shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let group = super::output::output_bind_group(device, &meter_layout, view, params, eye);
        Self { pipeline, group }
    }

    pub(super) fn resize(
        &mut self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        params: &wgpu::Buffer,
        eye: &wgpu::Buffer,
    ) {
        self.group = super::output::output_bind_group(
            device,
            &self.pipeline.get_bind_group_layout(1),
            view,
            params,
            eye,
        );
    }

    pub(super) fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &wgpu::BindGroup,
        shadows: &wgpu::BindGroup,
    ) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("measure scene luminance"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, frame, &[]);
        pass.set_bind_group(1, &self.group, &[]);
        pass.set_bind_group(2, shadows, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
}
