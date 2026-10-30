#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerCommand {
    Help,
    Status,
    Players,
    Metrics,
    Save,
    Stop,
}

impl ServerCommand {
    pub const ALL: [Self; 6] = [
        Self::Help,
        Self::Status,
        Self::Players,
        Self::Metrics,
        Self::Save,
        Self::Stop,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Help => "help",
            Self::Status => "status",
            Self::Players => "players list",
            Self::Metrics => "metrics show",
            Self::Save => "world save",
            Self::Stop => "stop",
        }
    }

    pub fn parse(line: &str) -> Result<Self, String> {
        if line.split_whitespace().next().is_none() {
            return Err("empty command".into());
        }
        Self::ALL
            .into_iter()
            .find(|command| {
                command
                    .name()
                    .split_whitespace()
                    .eq(line.split_whitespace())
            })
            .ok_or_else(|| "unknown command; use `help`".into())
    }
}

pub fn server_help() -> String {
    let mut help = String::with_capacity(64);
    for command in ServerCommand::ALL {
        if !help.is_empty() {
            help.push_str(" | ");
        }
        help.push_str(command.name());
    }
    help
}

pub fn control_usage_commands() -> String {
    let mut names = String::with_capacity(64);
    for command in ServerCommand::ALL
        .into_iter()
        .cycle()
        .skip(1)
        .take(ServerCommand::ALL.len())
    {
        if !names.is_empty() {
            names.push('|');
        }
        names.push_str(command.name());
    }
    names
}
