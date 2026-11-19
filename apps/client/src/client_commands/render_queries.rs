//! Queries served from one prepared local presentation snapshot.
use super::{presentation::DeveloperOperation, FrontendAction, PresentationOperation};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RenderQuery {
    Frames,
    Sky,
    Bodies,
    Terrain,
    Atmosphere,
    Weather,
    CloudTargets,
}

impl RenderQuery {
    pub(crate) fn from_action(action: &FrontendAction) -> Option<Self> {
        Some(match action {
            FrontendAction::FpsShow => Self::Frames,
            FrontendAction::Presentation(PresentationOperation::SkyInfo) => Self::Sky,
            FrontendAction::Presentation(PresentationOperation::SkyBodies) => Self::Bodies,
            FrontendAction::Presentation(PresentationOperation::TerrainStatus) => Self::Terrain,
            FrontendAction::Presentation(PresentationOperation::AtmosphereStatus) => {
                Self::Atmosphere
            }
            FrontendAction::Presentation(PresentationOperation::WeatherStatus) => Self::Weather,
            FrontendAction::Presentation(PresentationOperation::Developer(
                DeveloperOperation::Clouds,
            )) => Self::CloudTargets,
            _ => return None,
        })
    }
}
