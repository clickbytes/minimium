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
        let words: Vec<_> = line.split_whitespace().collect();
        match words.as_slice() {
            ["help"] => Ok(Self::Help),
            ["status"] => Ok(Self::Status),
            ["players", "list"] => Ok(Self::Players),
            ["metrics", "show"] => Ok(Self::Metrics),
            ["world", "save"] => Ok(Self::Save),
            ["stop"] => Ok(Self::Stop),
            [] => Err("empty command".into()),
            _ => Err("unknown command; use `help`".into()),
        }
    }
}

pub const SERVER_HELP: &str = "help | status | players list | metrics show | world save | stop";
