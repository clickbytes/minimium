use crate::{PropertyLifecycle, PropertySource, PropertyValue};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ReadSource {
    LocalOwner,
    OwnerSnapshot,
    Replica,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Freshness {
    Current,
    Sampled,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ConfigurationSource {
    Default,
    File,
    Environment,
    CommandLine,
}

impl ConfigurationSource {
    pub fn from_source(source: PropertySource) -> Option<Self> {
        Some(match source {
            PropertySource::Default => Self::Default,
            PropertySource::File => Self::File,
            PropertySource::Environment => Self::Environment,
            PropertySource::CommandLine => Self::CommandLine,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::File => "file",
            Self::Environment => "environment",
            Self::CommandLine => "command_line",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StartupValue<T> {
    pub value: T,
    pub source: ConfigurationSource,
}

/// Projection of a resolved startup layer from the same property report.
pub struct ConfigurationValue<'a, T> {
    pub startup: &'a StartupValue<T>,
    pub factory_default: Option<&'a T>,
    pub lifecycle: PropertyLifecycle,
}

impl<T> PropertyValue<T> {
    pub fn configuration(&self) -> Option<ConfigurationValue<'_, T>> {
        Some(ConfigurationValue {
            startup: self.startup.as_ref()?,
            factory_default: self.default.as_ref(),
            lifecycle: self.lifecycle,
        })
    }
}

impl<T> PropertyValue<T> {
    pub fn format_with(&self, render: impl Fn(&T) -> String) -> String {
        let encode = |v: Option<&T>| v.map(&render).unwrap_or_else(|| "unavailable".into());
        let source = match self.read_source {
            ReadSource::LocalOwner => "local_owner",
            ReadSource::OwnerSnapshot => "owner_snapshot",
            ReadSource::Replica => "replica",
            ReadSource::Unavailable => "unavailable",
        };
        let freshness = match self.freshness {
            Freshness::Current => "current",
            Freshness::Sampled => "sampled",
            Freshness::Unknown => "unknown",
        };
        let config = self.configuration();
        let requested = encode(self.requested.as_ref());
        let effective = encode(self.effective.as_ref());
        let default = encode(self.default.as_ref());
        let startup = encode(config.as_ref().map(|c| &c.startup.value));
        let startup_source = config
            .as_ref()
            .map_or("unavailable", |c| c.startup.source.label());
        let session = self
            .session_override
            .as_ref()
            .map(&render)
            .unwrap_or_else(|| "none".into());
        let revision = self
            .revision
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unavailable".into());
        let reason = self.difference_reason.as_deref().unwrap_or("none");
        let lifecycle = match self.lifecycle {
            PropertyLifecycle::Live => "live",
            PropertyLifecycle::ReadOnly => "read_only",
            PropertyLifecycle::CreationOnly => "creation_only",
            PropertyLifecycle::RestartRequired => "restart_required",
        };
        format!("requested={requested} effective={effective} default={default} startup={startup} startup_source={startup_source} session_override={session} source={source} revision={revision} freshness={freshness} lifecycle={lifecycle} reason={reason}")
    }
}
