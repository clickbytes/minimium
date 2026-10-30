//! Shared, renderer-independent sky and observer contracts.

mod observer;

pub use observer::{
    normalized_observer_direction, resolve_sky_model, try_observer_frame, EarthHeightReference,
    GeographicObserver, ObserverError, ObserverFrame, ObserverInput, SkyModel,
    DEFAULT_OBSERVER_EYE_HEIGHT_MM, HEIGHT_REFERENCE_REVISION, MAX_OBSERVER_EYE_HEIGHT_MM,
    MAX_OBSERVER_POSITION_MM,
};
