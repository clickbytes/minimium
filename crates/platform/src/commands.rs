#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerCommand {
    Help,
    Status,
    Players,
    Metrics,
    Save,
    Stop,
}

impl ServerCommand {
    pub fn parse(line: &str) -> Result<Self, String> {
        let mut words = line.split_whitespace();
        let command = match (words.next(), words.next(), words.next()) {
            (None, None, None) => return Err("empty command".into()),
            (Some("help"), None, None) => Self::Help,
            (Some("status"), None, None) => Self::Status,
            (Some("players"), Some("list"), None) => Self::Players,
            (Some("metrics"), Some("show"), None) => Self::Metrics,
            (Some("world"), Some("save"), None) => Self::Save,
            (Some("stop"), None, None) => Self::Stop,
            _ => return Err("unknown command; use `help`".into()),
        };
        if words.next().is_some() {
            return Err("unknown command; use `help`".into());
        }
        Ok(command)
    }
}

pub const SERVER_HELP: &str = "help | status | players list | metrics show | world save | stop";
