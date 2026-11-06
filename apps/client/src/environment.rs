//! Application-only demo inputs and local lakes.
#[cfg(feature = "desktop")]
pub(crate) mod demo;
#[cfg(feature = "desktop")]
pub(crate) mod lakes;

#[cfg(feature = "desktop")]
pub(crate) use demo::LocalDemoEnvironment;
pub(crate) use minimium_client_runtime::environment::EnvironmentFrameInput;
#[cfg(feature = "desktop")]
pub(crate) use minimium_client_runtime::environment::SkyDiagnosticsSnapshot;
#[cfg(feature = "desktop")]
pub(crate) use minimium_client_runtime::environment::{diagnostics, SynchronizationQuality};
