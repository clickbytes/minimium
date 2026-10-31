//! Bounded three-layer global atmospheric circulation for a game planet.
//! World time and seed reconstruct the same state without camera history or GPU.
//! It is a parameterized fluid approximation, not an operational weather model.
mod dynamics;
mod grid;
mod profile;

pub use grid::{AtmosphericCell, WeatherGrid, WeatherSurface, GRID_HEIGHT, GRID_WIDTH, LEVELS};
pub use profile::{AtmosphereProfile, Condensable, GasComposition};
pub const MODEL_REVISION: u32 = 1;
