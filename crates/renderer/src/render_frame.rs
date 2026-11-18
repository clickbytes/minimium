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
    /// Selected lights are independent of the producer's visible emission geometry.
    pub transient_lights: crate::transient_lighting::TransientLightFrame,
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
            transient_lights: crate::transient_lighting::TransientLightFrame::empty(),
            client: None,
            ui_vertices: Vec::new(),
        }
    }

    /// Attaches the bounded meteor contract built for this prepared frame.
    pub fn with_meteors(mut self, meteors: MeteorFrame) -> Self {
        self.meteors = meteors;
        self
    }

    pub fn with_transient_lights(
        mut self,
        lights: crate::transient_lighting::TransientLightFrame,
    ) -> Self {
        self.transient_lights = lights;
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

    /// Pre-exposes physical radiance for storage in the HDR scene and atmosphere LUTs.
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
            // The catalog and galactic plate have an authored twilight contrast.
            // Convert from that reference view to scene units before HDR storage.
            // The final eye exposure still responds to bright ground and lunar glare.
            let reference = exposure_reference(self.view, environment.global_sun_direction, 1.0);
            environment.star_animation[1] = environment.ambient_rgb_exposure[3] / reference;
        } else {
            // The analytic route has no frame meter. Expose only the reflected
            // lunar radiance against its already display-calibrated sky background.
            let night = observer_night_fraction(self.view, environment.global_sun_direction);
            let [r, g, b, _] = environment.local_moon_rgb_visibility;
            let moon = glam::Vec4::from_array(environment.moon_apparent_world).truncate();
            let up = glam::Vec4::from_array(environment.observer_up_horizon_cos).truncate();
            let horizontal = luminance([r, g, b]) * moon.dot(up).max(0.0);
            // With no image meter, estimate a moonlit 80%-reflecting surface.
            // Keep bright snow within the display shoulder at high lunar altitude.
            let white = 0.8 * horizontal * environment.ambient_rgb_exposure[3];
            let ceiling = (NIGHT_REFERENCE_STOPS as f32 * night).exp2();
            environment.star_animation[2] = if white > 0.0 {
                ceiling.min((0.55 / white).max(1.0))
            } else {
                ceiling
            };
        }
        environment
    }
}

// At 14 stops the Earth soil material under a full Moon 30 degrees high has
// only one sRGB code value of direct light. The meter needs headroom for
// diffuse brown/green surfaces, even when the stars are already visible.
const NIGHT_REFERENCE_STOPS: u32 = 21;

/// Photometric luminance of linear RGB in the scene's solar-relative units.
pub fn luminance([red, green, blue]: [f32; 3]) -> f32 {
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}

fn night_fraction(solar_cosine: f64) -> f32 {
    let altitude = solar_cosine.clamp(-1.0, 1.0).asin().to_degrees();
    let t = ((-altitude - 6.0) / 12.0).clamp(0.0, 1.0);
    (t * t * (3.0 - 2.0 * t)) as f32
}

fn orbital_fraction(view: crate::RenderViewFrame) -> f32 {
    if view.game_planet_radius_m <= 0.0 {
        return 0.0;
    }
    // At altitude the same observer can face either sunlit ground or empty space.
    let distance = glam::DVec3::from_array(view.camera_eye_world_m).length();
    let t = ((distance / view.game_planet_radius_m - 1.01) / 0.02).clamp(0.0, 1.0);
    (t * t * (3.0 - 2.0 * t)) as f32
}

fn observer_night_fraction(view: crate::RenderViewFrame, sun: [f32; 4]) -> f32 {
    let up = glam::DVec3::from_array(view.camera_eye_world_m).normalize_or_zero();
    let direction = glam::Vec4::from_array(sun)
        .truncate()
        .as_dvec3()
        .normalize_or_zero();
    night_fraction(up.dot(direction))
}

/// Adaptation prior for an Earth observer. It depends on position and time so
/// turning in place changes the meter's image input without moving its reference.
pub fn exposure_reference(view: crate::RenderViewFrame, sun: [f32; 4], base: f32) -> f32 {
    let night = observer_night_fraction(view, sun);
    let darkness = night + (1.0 - night) * orbital_fraction(view);
    base * (NIGHT_REFERENCE_STOPS as f32 * darkness).exp2()
}

/// Storage gain. A visible daylight limb keeps orbital radiance representable,
/// while dark views gain precision before half-float scene and LUT storage.
pub fn view_exposure(view: crate::RenderViewFrame, sun: [f32; 4], base: f32) -> f32 {
    let direction =
        glam::DVec3::new(sun[0] as f64, sun[1] as f64, sun[2] as f64).normalize_or_zero();
    let mut points = crate::shadow_view::surface_footprint(view);
    points.push(glam::DVec3::from_array(view.camera_eye_world_m));
    let cosine = points
        .iter()
        .map(|p| p.normalize_or_zero().dot(direction))
        .fold(-1.0, f64::max);
    base * (15.0 * night_fraction(cosine)).exp2()
}
