// Protocol E004-04/05 will provide the server descriptor and context to this adapter.
use minimium_core::{
    environment::EnvironmentDescriptor,
    sky::{
        resolve_sky_model, try_observer_frame, ObserverError, ObserverFrame, ObserverInput,
        SkyModel,
    },
    world::{Position3Mm, WorldManifest, WorldTopology},
};
use minimium_protocol::WorldContext;

#[allow(dead_code)]
#[derive(Default)]
pub struct ObserverAdapter {
    scope: Option<ObserverScope>,
    frame: Option<ObserverFrame>,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ObserverScope {
    context: WorldContext,
    manifest: WorldManifest,
}

#[allow(dead_code)]
impl ObserverAdapter {
    /// Builds the player observer from precise game state, independent of the active camera.
    pub fn update(
        &mut self,
        context: WorldContext,
        manifest: WorldManifest,
        descriptor: EnvironmentDescriptor,
        feet_position_mm: Option<Position3Mm>,
        eye_height_mm: u32,
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
        let result = self.build_frame(manifest, descriptor, feet_position_mm, eye_height_mm);
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

    pub fn frame(&self) -> Option<ObserverFrame> {
        self.frame
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
    ) -> Result<Option<ObserverFrame>, ObserverError> {
        if manifest.topology() != WorldTopology::Planet
            && descriptor.profile() != minimium_core::environment::EnvironmentProfile::Interior
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
