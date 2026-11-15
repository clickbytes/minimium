use super::{environment::PreparedEnvironmentFrame, shadow_prepare::PreparedShadowFrame};
use minimium_renderer::meteors::MeteorFrame;
use minimium_renderer::world_geometry_gpu::GeometryPublication;

/// Immutable value and read-only world inputs consumed by the GPU renderer.
pub(super) struct RenderSceneSnapshot<'a> {
    pub(super) bolids: minimium_renderer::milky_way_gpu::BolidGpuFrame,
    pub(super) environment: &'a PreparedEnvironmentFrame,
    pub(super) shadow: &'a PreparedShadowFrame,
    pub(super) publication: Option<GeometryPublication>,
    pub(super) context: Option<minimium_protocol::WorldContext>,
}

impl<'a> RenderSceneSnapshot<'a> {
    pub(super) fn new(
        bolids: minimium_renderer::milky_way_gpu::BolidGpuFrame,
        environment: &'a PreparedEnvironmentFrame,
        shadow: &'a PreparedShadowFrame,
        publication: Option<GeometryPublication>,
        context: Option<minimium_protocol::WorldContext>,
    ) -> Self {
        Self {
            bolids,
            environment,
            shadow,
            publication,
            context,
        }
    }

    /// The bounded per-frame meteor contract of the prepared environment.
    pub(super) fn meteors(&self) -> MeteorFrame {
        self.environment.render_frame.meteors
    }
}
