use minimium_environment::sky::{
    resolve_sky_model, try_observer_frame, ObserverError, ObserverFrame, ObserverInput, SkyModel,
};
use minimium_environment::EnvironmentDescriptor;
use minimium_protocol::WorldContext;
use minimium_world::world::{Position3Mm, WorldManifest, WorldTopology};

#[derive(Default)]
pub struct ObserverAdapter {
    scope: Option<ObserverScope>,
    frame: Option<ObserverFrame>,
}

/// Render-only reference for the bounded astronomy API. The optical model keeps
/// the actual uncapped camera separately, including its game-to-physical scale.
/// The meteor surface scale is resolved here at the same instant (same
/// sampled reference, no duplicated per-frame work) and threaded into the
/// environment frame for the client meteor runtime (ADR 0013 § 4).
pub(super) fn camera_reference(
    manifest: WorldManifest,
    view: minimium_renderer::RenderViewFrame,
    geography: Option<&dyn minimium_procgen::PreparedGeography>,
) -> Result<
    (
        Position3Mm,
        minimium_renderer::meteors::coords::PlanetSurfaceScale,
    ),
    ObserverError,
> {
    let eye = glam::DVec3::from_array(view.camera_eye_world_m);
    if !eye.is_finite() || eye.length() < 1.0 {
        return Err(ObserverError::NonFinite);
    }
    let radius = view.game_planet_radius_m;
    let actual = Position3Mm(eye.to_array().map(|v| (v * 1000.0).round() as i64));
    let (offset, meteor_surface_scale) = match manifest.planet() {
        Some(definition) if definition.mode == 1 => {
            let reference =
                minimium_procgen::earth_height_reference(actual, definition, geography)?;
            let scale = if definition.peak_relief.enabled() {
                minimium_renderer::meteors::coords::PlanetSurfaceScale::from_earth(
                    definition.height_scale_permille,
                    reference,
                )
            } else {
                minimium_renderer::meteors::coords::PlanetSurfaceScale::from_permille(
                    definition.height_scale_permille,
                )
            }
            .ok_or(ObserverError::InvalidPlanet)?;
            let physical_height = scale
                .physical_altitude_m(eye.length() - radius)
                .clamp(-1000.0, 80000.0);
            let offset = reference.baseline_game_mm as f64 / 1000.0 + physical_height
                - reference.physical_anchor_m;
            (offset, scale)
        }
        // Procedural planets compress elevations with the same manifest field
        // (`earth_elevation_mm` scales by `height_scale_permille`), so the
        // meteor mapping reuses it instead of assuming a constant.
        Some(other) => {
            let scale = minimium_renderer::meteors::coords::PlanetSurfaceScale::from_permille(
                other.height_scale_permille,
            )
            .ok_or(ObserverError::InvalidPlanet)?;
            ((eye.length() - radius).clamp(-1000.0, 80000.0), scale)
        }
        // No planet definition: the radial game metres stay 1:1 physical.
        None => (
            (eye.length() - radius).clamp(-1000.0, 80000.0),
            minimium_renderer::meteors::coords::PlanetSurfaceScale::UNIT,
        ),
    };
    Ok((
        Position3Mm(
            (eye.normalize() * (radius + offset))
                .to_array()
                .map(|v| (v * 1000.0).round() as i64),
        ),
        meteor_surface_scale,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ObserverScope {
    context: WorldContext,
    manifest: WorldManifest,
}

impl ObserverAdapter {
    /// Builds an observer from a precise supplied position. Render callers may
    /// supply a camera reference; authoritative player observations stay separate.
    pub fn update(
        &mut self,
        context: WorldContext,
        manifest: WorldManifest,
        descriptor: EnvironmentDescriptor,
        feet_position_mm: Option<Position3Mm>,
        eye_height_mm: u32,
        geography: Option<&dyn minimium_procgen::PreparedGeography>,
    ) -> Result<Option<ObserverFrame>, ObserverError> {
        let scope = ObserverScope { context, manifest };
        if self.scope != Some(scope) {
            self.frame = None;
        }
        self.scope = Some(scope);
        if context.content_revision() != manifest.content_revision() {
            self.frame = None;
            return Err(ObserverError::EnvironmentProfileMismatch);
        }
        let result = self.build_frame(
            manifest,
            descriptor,
            feet_position_mm,
            eye_height_mm,
            geography,
        );
        match result {
            Ok(frame) => {
                self.frame = frame;
                Ok(frame)
            }
            Err(error) => {
                self.frame = None;
                Err(error)
            }
        }
    }

    pub fn clear(&mut self) {
        self.scope = None;
        self.frame = None;
    }

    fn build_frame(
        &self,
        manifest: WorldManifest,
        descriptor: EnvironmentDescriptor,
        feet_position_mm: Option<Position3Mm>,
        eye_height_mm: u32,
        geography: Option<&dyn minimium_procgen::PreparedGeography>,
    ) -> Result<Option<ObserverFrame>, ObserverError> {
        if manifest.topology() != WorldTopology::Planet
            && descriptor.profile() != minimium_environment::EnvironmentProfile::Interior
        {
            return Err(ObserverError::EnvironmentProfileMismatch);
        }
        let model = resolve_sky_model(manifest, descriptor)?;
        if model == SkyModel::None {
            return Ok(None);
        }
        let feet_position_mm = feet_position_mm.ok_or(ObserverError::MissingPrecisePosition)?;
        let definition = manifest.planet();
        let game_radius_mm = if let Some(definition) = definition {
            if !definition.is_valid() {
                return Err(ObserverError::InvalidPlanet);
            }
            definition.radius_mm
        } else {
            if manifest.topology() != WorldTopology::Planet {
                return Err(ObserverError::EnvironmentProfileMismatch);
            }
            minimium_procgen::planet_surface_for(manifest).radius_mm()
        };
        let height_reference = if model == SkyModel::EarthGeoreferencedV1 {
            let definition = definition.ok_or(ObserverError::InvalidPlanet)?;
            Some(minimium_procgen::earth_height_reference(
                feet_position_mm,
                definition,
                geography,
            )?)
        } else {
            None
        };
        try_observer_frame(ObserverInput {
            feet_position_mm,
            eye_height_mm,
            model,
            game_radius_mm,
            planet_definition: definition,
            height_reference,
        })
    }
}
