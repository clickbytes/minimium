//! Small static command metadata registry shared by interactive front ends.

/// One command declaration and its domain-owned parser.
pub struct CommandSpec<T, E> {
    pub name: &'static str,
    pub alias: Option<&'static str>,
    pub usage: &'static str,
    pub parser: fn(&[&str]) -> Result<T, E>,
}

impl<T, E> Copy for CommandSpec<T, E> {}

impl<T, E> Clone for CommandSpec<T, E> {
    fn clone(&self) -> Self {
        *self
    }
}

/// Resolves the longest registered command name or alias before parsing its tail.
pub struct CommandRegistry<T, E> {
    commands: Vec<CommandSpec<T, E>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandRegistryError {
    InvalidName(String),
    DuplicateName(String),
}

impl std::fmt::Display for CommandRegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName(name) => write!(formatter, "invalid command name: {name}"),
            Self::DuplicateName(name) => {
                write!(formatter, "duplicate command name or alias: {name}")
            }
        }
    }
}

impl std::error::Error for CommandRegistryError {}

/// Distinguishes an unrecognized command from a recognized command with bad arguments.
pub enum CommandOutcome<T, E> {
    Unknown,
    Matched {
        name: &'static str,
        usage: &'static str,
        result: Result<T, E>,
    },
}

impl<T, E> Default for CommandRegistry<T, E> {
    fn default() -> Self {
        Self {
            commands: Vec::new(),
        }
    }
}

impl<T, E> CommandRegistry<T, E> {
    /// Lexes and resolves a source line without losing the original raw tail.
    pub fn parse_line(&self, input: &str) -> Result<CommandOutcome<T, E>, crate::CommandError> {
        let tokens = crate::lex_command_line_with_spans(input)?;
        let words = tokens
            .iter()
            .map(|token| token.value.as_str())
            .collect::<Vec<_>>();
        let matched = self
            .commands
            .iter()
            .filter_map(|spec| {
                let count = [Some(spec.name), spec.alias]
                    .into_iter()
                    .flatten()
                    .map(|name| name.split_whitespace().collect::<Vec<_>>())
                    .filter(|name| words.starts_with(name))
                    .map(|name| name.len())
                    .max()?;
                Some((spec, count))
            })
            .max_by_key(|(_, count)| *count);
        let Some((spec, command_length)) = matched else {
            return Ok(CommandOutcome::Unknown);
        };
        let tail = tokens
            .get(command_length)
            .map_or("", |token| input[token.span.start..].trim_start());
        let arguments = [spec.name, tail];
        Ok(CommandOutcome::Matched {
            name: spec.name,
            usage: spec.usage,
            result: (spec.parser)(&arguments),
        })
    }

    pub fn register(&mut self, specs: &'static [CommandSpec<T, E>]) {
        self.try_register(specs)
            .unwrap_or_else(|error| panic!("command registration failed: {error}"));
    }

    /// Adds all declarations atomically after checking names and aliases together.
    pub fn try_register(
        &mut self,
        specs: &'static [CommandSpec<T, E>],
    ) -> Result<(), CommandRegistryError> {
        let mut staged_names = Vec::new();
        for spec in specs {
            for name in std::iter::once(spec.name).chain(spec.alias) {
                if !valid_command_name(name) {
                    return Err(CommandRegistryError::InvalidName(name.to_owned()));
                }
                let duplicate = self
                    .commands
                    .iter()
                    .flat_map(|existing| std::iter::once(existing.name).chain(existing.alias))
                    .chain(staged_names.iter().copied())
                    .any(|registered| registered == name);
                if duplicate {
                    return Err(CommandRegistryError::DuplicateName(name.to_owned()));
                }
                staged_names.push(name);
            }
        }
        self.commands.extend(specs.iter().copied());
        Ok(())
    }

    pub fn parse(&self, input: &str, words: &[&str]) -> Option<Result<T, (&'static str, E)>> {
        match self.parse_outcome(input, words) {
            CommandOutcome::Unknown => None,
            CommandOutcome::Matched { usage, result, .. } => {
                Some(result.map_err(|error| (usage, error)))
            }
        }
    }

    pub fn parse_outcome(&self, input: &str, words: &[&str]) -> CommandOutcome<T, E> {
        self.commands
            .iter()
            .filter_map(|spec| {
                let matched = [Some(spec.name), spec.alias]
                    .into_iter()
                    .flatten()
                    .map(|name| name.split_whitespace().collect::<Vec<_>>())
                    .filter(|name| words.starts_with(name))
                    .max_by_key(Vec::len)?;
                Some((spec, matched.len()))
            })
            .max_by_key(|(_, length)| *length)
            .map(|(spec, command_length)| CommandOutcome::Matched {
                name: spec.name,
                usage: spec.usage,
                result: {
                    let arguments = [spec.name, tail_after_tokens(input, command_length)];
                    (spec.parser)(&arguments)
                },
            })
            .unwrap_or(CommandOutcome::Unknown)
    }

    pub fn commands(&self) -> &[CommandSpec<T, E>] {
        &self.commands
    }
}

fn valid_command_name(name: &str) -> bool {
    let words = name.split(' ').collect::<Vec<_>>();
    !words.is_empty()
        && words.iter().all(|word| {
            !word.is_empty()
                && word.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte)
                })
        })
}

fn tail_after_tokens(input: &str, count: usize) -> &str {
    let mut tail = input;
    for _ in 0..count {
        tail = tail.trim_start();
        let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        tail = &tail[end..];
    }
    tail.trim()
}
