//! Runtime-level connection options that are independent of client presentation.

/// Server worker lane selection for a local-server connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadLane {
    One,
    Two,
    Auto,
}

impl ThreadLane {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "1" => Ok(Self::One),
            "2" => Ok(Self::Two),
            "auto" => Ok(Self::Auto),
            _ => Err("--threads must be 1, 2, or auto".to_owned()),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::Two => "2",
            Self::Auto => "auto",
        }
    }
}
