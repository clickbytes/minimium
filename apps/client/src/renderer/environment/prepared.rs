use super::{EnvironmentFrame, StarFrameStamp};
use crate::renderer::{environment_gpu, GpuResources};
use minimium_renderer::meteors::MeteorFrame;
use minimium_renderer::stars::{StarInstanceGpu, STAR_GPU_RECORD_COUNT};
use minimium_renderer::RenderFrame;

/// One immutable CPU result, consumed only after acquiring a surface frame.
/// A skipped frame drops this bounded batch; the unchanged GPU stamp requests
/// preparation again on the next frame instead of publishing an unuploaded epoch.
pub(in crate::renderer) struct PreparedEnvironmentFrame {
    pub(in crate::renderer) frame: EnvironmentFrame,
    pub(in crate::renderer) render_frame: RenderFrame,
    stars: Option<PreparedStarUpload>,
}

pub(in crate::renderer) struct PreparedStarUpload {
    pub(in crate::renderer) instances: Box<[StarInstanceGpu; STAR_GPU_RECORD_COUNT]>,
    pub(in crate::renderer) stamp: StarFrameStamp,
}

impl PreparedEnvironmentFrame {
    /// The meteor frame is built once per frame by the client-local runtime
    /// (`crate::renderer::meteors`) and consumed here; the environment frame
    /// still carries the manifest-resolved surface scale at the same seam.
    pub(in crate::renderer) fn new(
        frame: EnvironmentFrame,
        stars: Option<PreparedStarUpload>,
        meteors: MeteorFrame,
    ) -> Self {
        let uniform = environment_gpu::environment_uniform(&frame);
        let render_frame = RenderFrame::new(
            frame.render_view,
            frame.camera_uniform,
            uniform,
            frame.halo_count,
        )
        .with_meteors(meteors);
        Self {
            frame,
            render_frame,
            stars,
        }
    }

    pub(in crate::renderer) fn with_ui_vertices(
        mut self,
        vertices: Vec<minimium_renderer::UiVertex>,
    ) -> Self {
        self.render_frame = self.render_frame.with_ui_vertices(vertices);
        self
    }

    /// Maps the runtime's immutable client view into this renderer-owned frame.
    pub(in crate::renderer) fn with_client_view(
        mut self,
        view: &minimium_client_runtime::ClientView<'_>,
    ) -> Self {
        self.render_frame = self.render_frame.with_client_state(
            minimium_renderer::render_frame::ClientFrameState {
                world_instance: view.world_instance,
                player_position_m: view.player_position_m,
                health: view.health,
                sectors_ready: view.sectors_ready,
            },
        );
        self
    }

    pub(in crate::renderer) fn take_ui_vertices(&mut self) -> Vec<minimium_renderer::UiVertex> {
        self.render_frame.take_ui_vertices()
    }

    /// Uploads the app-owned, epoch-stamped star batch when it matches this frame.
    pub(in crate::renderer) fn upload_stars(&self, gpu: &mut GpuResources) {
        if let Some(stars) = &self.stars {
            if self.frame.diagnostics.star_frame == Some(stars.stamp) {
                gpu.stars.upload(&gpu.queue, &stars.instances);
                gpu.uploaded_star_stamp = Some(stars.stamp);
            }
        }
    }
}
