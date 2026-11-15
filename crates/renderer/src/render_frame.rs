//! Immutable CPU-owned presentation data for one frame.
use crate::{
    gpu_frame::{CameraUniform, EnvironmentUniform},
    meteors::{MeteorDisabledReason, MeteorFrame},
    RenderViewFrame, UiVertex,
};
use minimium_world::world::WorldInstanceId;

/// GPU-independent client world context adapted from `ClientView` for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClientFrameState {
    pub world_instance: WorldInstanceId,
    pub player_position_m: [f64; 3],
    pub health: u16,
    pub sectors_ready: bool,
}

/// Prepared camera, environment and overlay values consumed by GPU submission.
/// The frame owns CPU values only and never carries GPU resource handles.
pub struct RenderFrame {
    pub view: RenderViewFrame,
    pub camera: CameraUniform,
    pub environment: EnvironmentUniform,
    pub halo_count: u32,
    /// Bounded CPU-only meteor contract (ADR 0013); carried explicitly even
    /// while zero active so later passes consume one stable seam.
    pub meteors: MeteorFrame,
    /// Client presentation context attached by the application adapter.
    pub client: Option<ClientFrameState>,
    ui_vertices: Vec<UiVertex>,
}

impl RenderFrame {
    /// Creates a frame with prepared camera/environment values and no overlay geometry.
    pub fn new(
        view: RenderViewFrame,
        camera: CameraUniform,
        environment: EnvironmentUniform,
        halo_count: u32,
    ) -> Self {
        Self {
            view,
            camera,
            environment,
            halo_count,
            meteors: MeteorFrame::disabled(MeteorDisabledReason::NoEnvironment),
            client: None,
            ui_vertices: Vec::new(),
        }
    }

    /// Attaches the bounded meteor contract built for this prepared frame.
    pub fn with_meteors(mut self, meteors: MeteorFrame) -> Self {
        self.meteors = meteors;
        self
    }

    /// Attaches the immutable client state mapped from `ClientView` by the host.
    pub fn with_client_state(mut self, client: ClientFrameState) -> Self {
        self.client = Some(client);
        self
    }

    /// Returns a new frame carrying the overlay vertices prepared by the UI adapter.
    pub fn with_ui_vertices(mut self, vertices: Vec<UiVertex>) -> Self {
        self.ui_vertices = vertices;
        self
    }

    /// Returns overlay storage to the application for reuse after frame submission.
    pub fn take_ui_vertices(&mut self) -> Vec<UiVertex> {
        std::mem::take(&mut self.ui_vertices)
    }

    /// Borrows the prepared overlay values for the GPU adapter to upload.
    pub fn ui_vertices(&self) -> &[UiVertex] {
        &self.ui_vertices
    }

    /// Resolves output exposure once from immutable camera/lighting metadata.
    pub fn presentation_environment(
        &self,
        hdr_enabled: bool,
        physical_atmosphere: bool,
    ) -> EnvironmentUniform {
        let mut environment = self.environment;
        environment.flags[3] = u32::from(hdr_enabled);
        if hdr_enabled && physical_atmosphere {
            environment.ambient_rgb_exposure[3] = view_exposure(
                self.view,
                environment.global_sun_direction,
                environment.ambient_rgb_exposure[3],
            );
        }
        environment
    }
}

/// View-driven exposure, without a histogram/readback. A visible daylight limb
/// keeps orbital views in day exposure. Night gain preserves lunar irradiance
/// ratios; the HDR path applies it before half-float scene/LUT storage.
pub fn view_exposure(view: crate::RenderViewFrame, sun: [f32; 4], base: f32) -> f32 {
    let direction =
        glam::DVec3::new(sun[0] as f64, sun[1] as f64, sun[2] as f64).normalize_or_zero();
    let mut points = crate::shadow_view::surface_footprint(view);
    points.push(glam::DVec3::from_array(view.camera_eye_world_m));
    let cosine = points
        .iter()
        .map(|p| p.normalize_or_zero().dot(direction))
        .fold(-1.0, f64::max);
    let altitude = cosine.clamp(-1.0, 1.0).asin().to_degrees();
    let t = ((-altitude - 6.0) / 12.0).clamp(0.0, 1.0);
    let night = t * t * (3.0 - 2.0 * t);
    base * (15.0 * night as f32).exp2()
}
