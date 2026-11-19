//! Immutable catalog metadata. State and execution belong to domain owners.
mod catalog;
mod legacy;
mod spec;
mod values;

pub use catalog::{CatalogBuilder, CatalogError, OperationCatalog};
pub use legacy::{DomainBinding, LegacyPackage};
pub use spec::*;
pub use values::{CommandValue, FiniteValue};
