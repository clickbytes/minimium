use std::fmt;

use crate::time::{ClockId, TimeError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnvironmentError {
    Time(TimeError),
    InvalidProfile(u8),
    UnsupportedAstronomyModelRevision(u32),
    UnsupportedCatalogueId(u32),
    ClockMismatch { expected: u64, actual: u64 },
}

impl fmt::Display for EnvironmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Time(error) => fmt::Display::fmt(error, formatter),
            Self::InvalidProfile(value) => {
                write!(formatter, "unsupported environment profile {value}")
            }
            Self::UnsupportedAstronomyModelRevision(value) => {
                write!(formatter, "unsupported astronomy model revision {value}")
            }
            Self::UnsupportedCatalogueId(value) => {
                write!(formatter, "unsupported star catalogue id {value}")
            }
            Self::ClockMismatch { expected, actual } => write!(
                formatter,
                "environment clock id {actual} does not match expected clock id {expected}"
            ),
        }
    }
}

impl std::error::Error for EnvironmentError {}

impl From<TimeError> for EnvironmentError {
    fn from(error: TimeError) -> Self {
        Self::Time(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum EnvironmentProfile {
    Interior = 0,
    Earth = 1,
    EarthLike = 2,
}

impl EnvironmentProfile {
    pub const fn renders_sky(self) -> bool {
        !matches!(self, Self::Interior)
    }
}

impl TryFrom<u8> for EnvironmentProfile {
    type Error = EnvironmentError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Interior),
            1 => Ok(Self::Earth),
            2 => Ok(Self::EarthLike),
            other => Err(EnvironmentError::InvalidProfile(other)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AstronomyModelRevision(u32);

impl AstronomyModelRevision {
    pub const CURRENT: Self = Self(1);

    pub const fn new(value: u32) -> Result<Self, EnvironmentError> {
        if value == 1 {
            Ok(Self::CURRENT)
        } else {
            Err(EnvironmentError::UnsupportedAstronomyModelRevision(value))
        }
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for AstronomyModelRevision {
    type Error = EnvironmentError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogueId(u32);

impl CatalogueId {
    pub const CURRENT: Self = Self(2);

    pub const fn new(value: u32) -> Result<Self, EnvironmentError> {
        if value == 2 {
            Ok(Self::CURRENT)
        } else {
            Err(EnvironmentError::UnsupportedCatalogueId(value))
        }
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for CatalogueId {
    type Error = EnvironmentError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Opaque catalogue digest; core does not interpret or fabricate asset hashes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CatalogueSha256([u8; 32]);

impl CatalogueSha256 {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EnvironmentDescriptor {
    clock_id: ClockId,
    profile: EnvironmentProfile,
    astronomy_model_revision: AstronomyModelRevision,
    catalogue_id: CatalogueId,
    catalogue_sha256: CatalogueSha256,
}

impl EnvironmentDescriptor {
    pub const fn new(
        clock_id: ClockId,
        profile: EnvironmentProfile,
        astronomy_model_revision: AstronomyModelRevision,
        catalogue_id: CatalogueId,
        catalogue_sha256: CatalogueSha256,
    ) -> Self {
        Self {
            clock_id,
            profile,
            astronomy_model_revision,
            catalogue_id,
            catalogue_sha256,
        }
    }

    pub fn from_codes(
        clock_id: u64,
        profile: u8,
        astronomy_model_revision: u32,
        catalogue_id: u32,
        catalogue_sha256: [u8; 32],
    ) -> Result<Self, EnvironmentError> {
        Ok(Self::new(
            ClockId::try_from(clock_id)?,
            EnvironmentProfile::try_from(profile)?,
            AstronomyModelRevision::try_from(astronomy_model_revision)?,
            CatalogueId::try_from(catalogue_id)?,
            CatalogueSha256::new(catalogue_sha256),
        ))
    }

    pub const fn clock_id(self) -> ClockId {
        self.clock_id
    }

    pub const fn profile(self) -> EnvironmentProfile {
        self.profile
    }

    pub const fn astronomy_model_revision(self) -> AstronomyModelRevision {
        self.astronomy_model_revision
    }

    pub const fn catalogue_id(self) -> CatalogueId {
        self.catalogue_id
    }

    pub const fn catalogue_sha256(self) -> CatalogueSha256 {
        self.catalogue_sha256
    }

    pub fn validate_clock(&self, expected: ClockId) -> Result<(), EnvironmentError> {
        if self.clock_id == expected {
            Ok(())
        } else {
            Err(EnvironmentError::ClockMismatch {
                expected: expected.get(),
                actual: self.clock_id.get(),
            })
        }
    }
}
