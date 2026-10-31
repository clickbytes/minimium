//! Deterministic physical cloud field. No rendering or camera dependencies.
pub(crate) mod atmosphere;
pub(crate) mod coordinates;
pub(crate) mod density;
mod detail;
pub(crate) mod field;
mod lifecycle;
pub(crate) mod moments;
mod morphology;
mod motion;
mod population;
pub(crate) mod runtime;
pub(crate) mod sampling;
pub(crate) mod settings;
pub(crate) mod shapes;
mod spatial;
pub(crate) mod types;

pub(crate) use field::CloudFieldSnapshot;
pub(crate) use types::{CloudCluster, CloudId, CloudLobe};
pub(crate) const MODEL_REVISION: u64 = 3;
