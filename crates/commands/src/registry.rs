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

impl<T, E> Default for CommandRegistry<T, E> {
    fn default() -> Self {
        Self {
            commands: Vec::new(),
        }
    }
}

impl<T, E> CommandRegistry<T, E> {
    pub fn register(&mut self, specs: &'static [CommandSpec<T, E>]) {
        for spec in specs {
            for name in std::iter::once(spec.name).chain(spec.alias) {
                assert!(
                    !self.commands.iter().any(|existing| {
                        std::iter::once(existing.name)
                            .chain(existing.alias)
                            .any(|registered| registered == name)
                    }),
                    "duplicate command: {name}"
                );
            }
            self.commands.push(*spec);
        }
    }

    pub fn parse(&self, input: &str, words: &[&str]) -> Option<Result<T, (&'static str, E)>> {
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
            .map(|(spec, command_length)| {
                let arguments = [spec.name, tail_after_tokens(input, command_length)];
                (spec.parser)(&arguments).map_err(|error| (spec.usage, error))
            })
    }

    pub fn commands(&self) -> &[CommandSpec<T, E>] {
        &self.commands
    }
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
