//! Client-local bolid preview intents; no server operation is produced here.
use super::{
    presentation::{BolidPath, DeveloperOperation},
    FrontendAction, PresentationOperation,
};

#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
#[derive(Clone, Debug)]
pub(crate) enum BolidPreviewOperation {
    Transmission(bool),
    TransmissionRead,
    RestorePath(BolidPath),
    Fragments,
    Lighting,
    Status,
    Replay {
        age_seconds: f32,
        seed: Option<u64>,
        event_id: Option<u64>,
        frozen: bool,
    },
}

impl BolidPreviewOperation {
    pub(crate) fn from_action(action: &FrontendAction) -> Option<Self> {
        let FrontendAction::Presentation(PresentationOperation::Developer(operation)) = action
        else {
            return None;
        };
        Some(match operation {
            DeveloperOperation::Clouds => return None,
            DeveloperOperation::BolidTransmission(enabled) => Self::Transmission(*enabled),
            DeveloperOperation::BolidTransmissionRead => Self::TransmissionRead,
            DeveloperOperation::BolidPath(path) => Self::RestorePath(path.clone()),
            DeveloperOperation::BolidFragments => Self::Fragments,
            DeveloperOperation::BolidLighting => Self::Lighting,
            DeveloperOperation::BolidStatus => Self::Status,
            DeveloperOperation::BolidReplay {
                age_seconds,
                seed,
                event_id,
                frozen,
            } => Self::Replay {
                age_seconds: *age_seconds,
                seed: *seed,
                event_id: *event_id,
                frozen: *frozen,
            },
        })
    }
}
