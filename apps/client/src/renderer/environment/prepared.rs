use super::{
    stars::StarInstanceGpu, stars::STAR_GPU_RECORD_COUNT, EnvironmentFrame, StarFrameStamp,
};
use crate::renderer::{environment_gpu, GpuResources};

/// One immutable CPU result, consumed only after acquiring a surface frame.
/// A skipped frame drops this bounded batch; the unchanged GPU stamp requests
/// preparation again on the next frame instead of publishing an unuploaded epoch.
pub(in crate::renderer) struct PreparedEnvironmentFrame {
    pub(in crate::renderer) frame: EnvironmentFrame,
    uniform: environment_gpu::EnvironmentUniform,
    stars: Option<PreparedStarUpload>,
}

pub(in crate::renderer) struct PreparedStarUpload {
    pub(in crate::renderer) instances: Box<[StarInstanceGpu; STAR_GPU_RECORD_COUNT]>,
    pub(in crate::renderer) stamp: StarFrameStamp,
}

impl PreparedEnvironmentFrame {
    pub(in crate::renderer) fn new(
        frame: EnvironmentFrame,
        stars: Option<PreparedStarUpload>,
    ) -> Self {
        Self {
            uniform: environment_gpu::environment_uniform(&frame),
            frame,
            stars,
        }
    }

    pub(in crate::renderer) fn apply_cloud_light(&mut self, cover: [f32; 3]) {
        let ambient = (1.0 - cover[0] * 0.18 - cover[1] * 0.08).clamp(0.55, 1.0);
        for uniform in [&mut self.uniform, &mut self.frame.environment_uniform] {
            for channel in 0..3 {
                // Direct sunlight is attenuated by the shared spatial cloud shadow in WGSL.
                uniform.ambient_rgb_exposure[channel] *= ambient;
            }
        }
    }

    pub(in crate::renderer) fn apply_weather_fog(
        &mut self,
        amount: f32,
        moisture: f32,
        snow: bool,
    ) {
        let density = amount.clamp(0.0, 1.0).powi(2) * 0.035;
        for uniform in [&mut self.uniform, &mut self.frame.environment_uniform] {
            uniform.weather_fog = [density, 35.0, moisture.clamp(0.0, 1.0), f32::from(snow)];
        }
    }

    pub(in crate::renderer) fn apply_lightning_light(&mut self, intensity: f32) {
        for uniform in [&mut self.uniform, &mut self.frame.environment_uniform] {
            for (channel, blue) in [0.36, 0.42, 0.54].into_iter().enumerate() {
                uniform.ambient_rgb_exposure[channel] += blue * intensity;
            }
        }
    }

    /// CPU queue-write cost only: stars, camera and environment uniforms.
    /// Shadow work is measured separately; this is not GPU elapsed time.
    pub(in crate::renderer) fn upload(&self, gpu: &mut GpuResources) {
        if let Some(stars) = &self.stars {
            if self.frame.diagnostics.star_frame == Some(stars.stamp) {
                gpu.stars.upload(&gpu.queue, &stars.instances, stars.stamp);
            }
        }
        gpu.queue.write_buffer(
            &gpu.camera_buffer,
            0,
            bytemuck::bytes_of(&self.frame.camera_uniform),
        );
        let mut uniform = self.uniform;
        uniform.flags[3] = u32::from(gpu.hdr_enabled);
        gpu.queue
            .write_buffer(&gpu.environment_buffer, 0, bytemuck::bytes_of(&uniform));
        if let Some(output) = &gpu.output_encoder {
            output.update_exposure(
                &gpu.queue,
                self.frame.environment_uniform.ambient_rgb_exposure[3],
            );
        }
    }
}
