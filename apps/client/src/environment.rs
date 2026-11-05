//! Application-only demo inputs and local lakes.
pub(crate) mod demo;
pub(crate) mod lakes;

pub(crate) use demo::LocalDemoEnvironment;
pub(crate) use minimium_client_runtime::environment::{
    diagnostics, EnvironmentFrameInput, SkyDiagnosticsSnapshot, SynchronizationQuality,
};
