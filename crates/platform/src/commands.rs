mod time;

pub use time::{parse_scale_ppm, parse_world_time_utc, TimeCommand, TIME_USAGE};

const SERVER_COMMANDS: [&str; 6] = [
    "help",
    "status",
    "players list",
    "metrics show",
    "world save",
    "stop",
];

const SERVER_HELP: [&str; 13] = [
    "help",
    "status",
    "players list",
    "metrics show",
    "world save",
    "stop",
    "time status",
    "time mode real_time",
    "time mode time_scale",
    "time set RFC3339_UTC",
    "time scale DECIMAL",
    "time offline pause",
    "time offline advance",
];

const CONTROL_COMMANDS: [&str; 13] = [
    "help",
    "status",
    "players list",
    "metrics show",
    "world save",
    "stop",
    "time status",
    "time mode real_time",
    "time mode time_scale",
    "time set RFC3339_UTC",
    "time scale DECIMAL",
    "time offline pause",
    "time offline advance",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerCommand {
    Help,
    Status,
    Players,
    Metrics,
    Save,
    Stop,
    Time(TimeCommand),
    /// A validated administrative line owned by its registering server module.
    Registered(String),
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

    pub const fn name(&self) -> &'static str {
        match self {
            Self::Help => SERVER_COMMANDS[0],
            Self::Status => SERVER_COMMANDS[1],
            Self::Players => SERVER_COMMANDS[2],
            Self::Metrics => SERVER_COMMANDS[3],
            Self::Save => SERVER_COMMANDS[4],
            Self::Stop => SERVER_COMMANDS[5],
            Self::Time(_) => "time",
            Self::Registered(_) => "registered",
        }
    }

    pub fn parse(line: &str) -> Result<Self, String> {
        let words: Vec<_> = line.split_whitespace().collect();
        if words.is_empty() {
            return Err("empty command".into());
        }
        if words[0] == "time" {
            return TimeCommand::parse(&words[1..]).map(Self::Time);
        }
        SERVER_COMMANDS
            .iter()
            .zip(Self::ALL)
            .find_map(|(name, command)| {
                name.split_whitespace()
                    .eq(words.iter().copied())
                    .then_some(command)
            })
            .or_else(|| {
                (line.len() <= 512 && line.chars().all(|c| !c.is_control()))
                    .then(|| Self::Registered(line.trim().to_owned()))
            })
            .ok_or_else(|| "invalid command; use `help`".into())
    }

    pub const fn is_status(&self) -> bool {
        matches!(self, Self::Status)
    }

    pub const fn is_metrics(&self) -> bool {
        matches!(self, Self::Metrics)
    }

    pub const fn is_save_or_stop(&self) -> bool {
        matches!(self, Self::Save | Self::Stop)
    }

    pub const fn is_stop(&self) -> bool {
        matches!(self, Self::Stop)
    }
}

pub fn server_help() -> String {
    SERVER_HELP.join(" | ")
}

pub fn control_usage_commands() -> String {
    format!("{}|<module command>", CONTROL_COMMANDS.join("|"))
}
