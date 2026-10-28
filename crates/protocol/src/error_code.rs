/// A v8 error code. Unknown bytes are retained for forward compatibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerErrorCode {
    Protocol,
    Busy,
    ServerFull,
    RateLimit,
    Unknown(u8),
}

impl From<u8> for ServerErrorCode {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Protocol,
            2 => Self::Busy,
            3 => Self::ServerFull,
            4 => Self::RateLimit,
            other => Self::Unknown(other),
        }
    }
}

impl From<ServerErrorCode> for u8 {
    fn from(code: ServerErrorCode) -> Self {
        match code {
            ServerErrorCode::Protocol => 1,
            ServerErrorCode::Busy => 2,
            ServerErrorCode::ServerFull => 3,
            ServerErrorCode::RateLimit => 4,
            ServerErrorCode::Unknown(value) => value,
        }
    }
}
