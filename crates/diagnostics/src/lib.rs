//! Bounded diagnostic contracts shared by engine domains and applications.

use std::sync::Arc;

pub const MAX_EVENT_BYTES: usize = 8 * 1024;
pub const MAX_EVENT_FIELDS: usize = 32;
const BASE_EVENT_BYTES: usize = 192;
const MAX_CODE_BYTES: usize = 160;
const MAX_FIELD_KEY_BYTES: usize = 80;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DomainId {
    World,
    Simulation,
    Environment,
    Procgen,
    Sectors,
    Assets,
    AssetIo,
    Protocol,
    Transport,
    Storage,
    ClientRuntime,
    ServerRuntime,
    Renderer,
    Platform,
    GameSample,
}

impl DomainId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::Simulation => "simulation",
            Self::Environment => "environment",
            Self::Procgen => "procgen",
            Self::Sectors => "sectors",
            Self::Assets => "assets",
            Self::AssetIo => "asset_io",
            Self::Protocol => "protocol",
            Self::Transport => "transport",
            Self::Storage => "storage",
            Self::ClientRuntime => "client_runtime",
            Self::ServerRuntime => "server_runtime",
            Self::Renderer => "renderer",
            Self::Platform => "platform",
            Self::GameSample => "game_sample",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EventCode(&'static str);

impl EventCode {
    pub fn new(value: &'static str) -> Result<Self, EventCodeError> {
        if value.is_empty()
            || value.len() > MAX_CODE_BYTES
            || !value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            })
        {
            return Err(EventCodeError);
        }
        Ok(Self(value))
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventCodeError;

impl std::fmt::Display for EventCodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid stable diagnostic event code")
    }
}

impl std::error::Error for EventCodeError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticField {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticEvent {
    domain: DomainId,
    level: Level,
    code: EventCode,
    fields: Vec<DiagnosticField>,
    truncated: bool,
    encoded_budget: usize,
}

impl DiagnosticEvent {
    fn new(domain: DomainId, level: Level, code: EventCode) -> Self {
        Self {
            domain,
            level,
            code,
            fields: Vec::with_capacity(8),
            truncated: false,
            encoded_budget: BASE_EVENT_BYTES + code.as_str().len(),
        }
    }

    pub fn domain(&self) -> DomainId {
        self.domain
    }

    pub fn level(&self) -> Level {
        self.level
    }

    pub fn code(&self) -> EventCode {
        self.code
    }

    pub fn fields(&self) -> &[DiagnosticField] {
        &self.fields
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Add one field while keeping its allocation within the remaining event budget.
    pub fn field(mut self, key: &str, value: &str) -> Self {
        if self.fields.len() >= MAX_EVENT_FIELDS || self.encoded_budget >= MAX_EVENT_BYTES {
            self.truncated = true;
            return self;
        }
        let remaining_budget = MAX_EVENT_BYTES.saturating_sub(self.encoded_budget);
        let key_limit = MAX_FIELD_KEY_BYTES.min(remaining_budget.saturating_sub(8));
        if key_limit == 0 {
            self.truncated = true;
            return self;
        }
        let bounded_key = bounded_prefix(key, key_limit);
        self.truncated |= bounded_key.len() < key.len();
        let overhead = bounded_key.len().saturating_add(8);
        let remaining =
            MAX_EVENT_BYTES.saturating_sub(self.encoded_budget.saturating_add(overhead));
        let bounded_value = bounded_prefix(value, remaining);
        self.truncated |= bounded_value.len() < value.len();
        self.encoded_budget = self
            .encoded_budget
            .saturating_add(overhead)
            .saturating_add(bounded_value.len());
        self.fields.push(DiagnosticField {
            key: bounded_key.to_owned(),
            value: bounded_value.to_owned(),
        });
        self
    }
}

fn bounded_prefix(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

/// Implementations must accept or drop events without blocking the caller.
pub trait DiagnosticSink: Send + Sync + 'static {
    fn emit(&self, event: DiagnosticEvent);
}

#[derive(Clone)]
pub struct DomainLogger {
    domain: DomainId,
    sink: Arc<dyn DiagnosticSink>,
}

impl std::fmt::Debug for DomainLogger {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DomainLogger")
            .field("domain", &self.domain)
            .finish_non_exhaustive()
    }
}

impl DomainLogger {
    pub fn new(domain: DomainId, sink: Arc<dyn DiagnosticSink>) -> Self {
        Self { domain, sink }
    }

    pub fn domain(&self) -> DomainId {
        self.domain
    }

    pub fn event(&self, level: Level, code: EventCode) -> DiagnosticEvent {
        DiagnosticEvent::new(self.domain, level, code)
    }

    pub fn emit(&self, event: DiagnosticEvent) {
        if event.domain() == self.domain {
            self.sink.emit(event);
        }
    }
}
