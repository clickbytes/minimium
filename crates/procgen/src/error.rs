use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationErrorKind {
    InvalidManifest,
    UnsupportedRevision,
    InvalidBlueprint,
    Materialization,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationError {
    kind: GenerationErrorKind,
    detail: String,
}

impl GenerationError {
    pub fn invalid_manifest(detail: impl Into<String>) -> Self {
        Self::new(GenerationErrorKind::InvalidManifest, detail)
    }

    pub fn unsupported_revision(revision: u32) -> Self {
        Self::new(
            GenerationErrorKind::UnsupportedRevision,
            format!("generator revision {revision} is not valid for this definition"),
        )
    }

    pub fn invalid_blueprint(detail: impl Into<String>) -> Self {
        Self::new(GenerationErrorKind::InvalidBlueprint, detail)
    }

    pub fn materialization(detail: impl Into<String>) -> Self {
        Self::new(GenerationErrorKind::Materialization, detail)
    }

    pub fn kind(&self) -> GenerationErrorKind {
        self.kind
    }

    pub fn code(&self) -> &'static str {
        match self.kind {
            GenerationErrorKind::InvalidManifest => "procgen.invalid_manifest",
            GenerationErrorKind::UnsupportedRevision => "procgen.unsupported_revision",
            GenerationErrorKind::InvalidBlueprint => "procgen.invalid_blueprint",
            GenerationErrorKind::Materialization => "procgen.materialization_failed",
        }
    }

    fn new(kind: GenerationErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for GenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.detail)
    }
}

impl std::error::Error for GenerationError {}
