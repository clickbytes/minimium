use super::{EnvironmentFrame, EnvironmentFrameStatus, StarFrameStamp};
use crate::renderer::{GpuResources, environment_gpu};
use minimium_renderer::RenderFrame;
use minimium_renderer::stars::{STAR_GPU_RECORD_COUNT, StarInstanceGpu};

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
    pub(in crate::renderer) fn new(
        frame: EnvironmentFrame,
        stars: Option<PreparedStarUpload>,
    ) -> Self {
        let uniform = environment_gpu::environment_uniform(&frame);
        let context = minimium_renderer::meteors::MeteorFrameContext {
            camera_eye_world_m: frame.render_view.camera_eye_world_m,
            render_origin_m: frame.render_view.render_origin_m,
            game_radius_m: frame.render_view.game_planet_radius_m,
            // The neutral unit scale is exact while no sample has to traverse
            // the adapter; the runtime's resolved manifest scale plugs in at
            // the same seam when producers exist (E047-02).
            surface_scale: minimium_renderer::meteors::coords::PlanetSurfaceScale::UNIT,
        };
        let meteors = match frame.status {
            EnvironmentFrameStatus::Available => {
                minimium_renderer::meteors::MeteorFrame::ready(context)
            }
            EnvironmentFrameStatus::Interior => minimium_renderer::meteors::MeteorFrame::disabled(
                minimium_renderer::meteors::MeteorDisabledReason::Interior,
            ),
            EnvironmentFrameStatus::Unavailable => {
                minimium_renderer::meteors::MeteorFrame::disabled(
                    minimium_renderer::meteors::MeteorDisabledReason::NoEnvironment,
                )
            }
        };
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
