mod format;
mod record;

mod identity {
    include!("sky_catalog/identity.generated.rs");
}

use std::{error::Error, fmt, sync::OnceLock};

pub use record::{StarCatalogue, StarRecord};

static EMBEDDED_CATALOGUE: OnceLock<Result<StarCatalogue, CatalogError>> = OnceLock::new();
static EMBEDDED_BYTES: &[u8] = include_bytes!("../data/sky/stars-v1.bin");
static EMBEDDED_LABELS: &str = include_str!("../data/sky/star-labels-v1.tsv");
static PARSED_LABELS: OnceLock<Result<Box<[&'static str]>, CatalogError>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SkyCatalogIdentity {
    pub catalogue_id: u32,
    pub sha256: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogError {
    Length,
    Magic,
    UnsupportedSchema,
    IdentityMismatch,
    DigestMismatch,
    InvalidRecord {
        index: u32,
        reason: InvalidRecordReason,
    },
    InvalidCount,
    InvalidLabels,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidRecordReason {
    ZeroId,
    DuplicateId,
    NonFiniteMagnitude,
    MagnitudeOrder,
    NonFiniteColor,
    NonFinitePosition,
    ZeroPosition,
    NonFiniteVelocity,
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length => formatter.write_str("sky catalogue has an invalid length"),
            Self::Magic => formatter.write_str("sky catalogue magic is invalid"),
            Self::UnsupportedSchema => formatter.write_str("sky catalogue schema is unsupported"),
            Self::IdentityMismatch => formatter.write_str("sky catalogue identity does not match"),
            Self::DigestMismatch => formatter.write_str("sky catalogue digest does not match"),
            Self::InvalidRecord { index, reason } => {
                write!(
                    formatter,
                    "sky catalogue record {index} is invalid: {reason:?}"
                )
            }
            Self::InvalidCount => {
                formatter.write_str("sky catalogue must contain exactly 1000 records")
            }
            Self::InvalidLabels => {
                formatter.write_str("sky catalogue labels do not match its records")
            }
        }
    }
}

impl Error for CatalogError {}

pub fn embedded_sky_catalog_identity() -> SkyCatalogIdentity {
    SkyCatalogIdentity {
        catalogue_id: identity::CATALOGUE_ID,
        sha256: identity::ASSET_SHA256,
    }
}

pub fn embedded_sky_catalog() -> Result<&'static StarCatalogue, CatalogError> {
    match EMBEDDED_CATALOGUE.get_or_init(|| format::decode(EMBEDDED_BYTES)) {
        Ok(catalogue) => Ok(catalogue),
        Err(error) => Err(*error),
    }
}

pub fn embedded_star_labels() -> Result<&'static [&'static str], CatalogError> {
    match PARSED_LABELS.get_or_init(|| {
        let catalogue = embedded_sky_catalog()?;
        let labels = EMBEDDED_LABELS.lines().collect::<Vec<_>>();
        if labels.len() != catalogue.records().len() {
            return Err(CatalogError::InvalidLabels);
        }
        labels
            .into_iter()
            .zip(catalogue.records())
            .map(|(line, record)| {
                let (id, label) = line.split_once('\t').ok_or(CatalogError::InvalidLabels)?;
                if id.parse::<u32>().ok() != Some(record.hyg_id) || label.is_empty() {
                    return Err(CatalogError::InvalidLabels);
                }
                Ok(label)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Vec::into_boxed_slice)
    }) {
        Ok(labels) => Ok(labels),
        Err(error) => Err(*error),
    }
}
