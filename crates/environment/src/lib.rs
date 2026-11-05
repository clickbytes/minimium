//! Renderer-independent sky observation and astronomical catalogue contracts.

mod commands;
mod model;
pub mod time;

pub use commands::{parse_scale_ppm, parse_world_time_utc, TimeCommand, TIME_USAGE};
pub use model::{
    AstronomyModelRevision, CatalogueId, CatalogueSha256, EnvironmentDescriptor, EnvironmentError,
    EnvironmentProfile,
};

pub mod sky;
pub mod universe;
pub mod weather;
