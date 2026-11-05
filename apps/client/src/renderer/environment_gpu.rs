use super::environment::{EnvironmentFrame, EnvironmentFrameStatus};
use minimium_renderer::gpu_frame::EnvironmentUniform;

pub(super) fn environment_uniform(frame: &EnvironmentFrame) -> EnvironmentUniform {
    minimium_renderer::gpu_frame::environment_uniform(
        frame.environment_uniform,
        frame.render_view,
        frame.status != EnvironmentFrameStatus::Unavailable,
        frame.halo_count,
        frame.halo_sources,
    )
}
