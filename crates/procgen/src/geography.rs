use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EarthSampleError {
    PackageUnavailable,
    PackageMismatch,
    InvalidDirection,
}

impl fmt::Display for EarthSampleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PackageUnavailable => "GEBCO package is not prepared",
            Self::PackageMismatch => "prepared GEBCO package does not match the planet",
            Self::InvalidDirection => "Earth sample direction is zero or non-finite",
        })
    }
}

impl std::error::Error for EarthSampleError {}

/// Immutable host-prepared geographic data used by deterministic world algorithms.
pub trait PreparedGeography: Send + Sync + std::fmt::Debug {
    fn land_cover(&self, _direction: [f64; 3]) -> Option<[f32; 6]> {
        None
    }
    fn package_digest(&self) -> [u8; 32];
    fn elevation_m(&self, direction: [f64; 3]) -> Result<f64, EarthSampleError>;
    fn elevation_upper_bound_m(
        &self,
        direction: [f64; 3],
        angle: f64,
    ) -> Result<f64, EarthSampleError>;
}
