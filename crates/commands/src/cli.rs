use crate::{CommandError, CommandErrorCode};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CliOptionSpec {
    /// Long spelling, including the leading `--`.
    pub name: &'static str,
    /// Optional short spelling, including the leading `-`.
    pub alias: Option<&'static str>,
    pub value_name: Option<&'static str>,
    pub description: &'static str,
    pub repeatable: bool,
    pub help: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliValues {
    values: BTreeMap<&'static str, Vec<Option<String>>>,
    positionals: Vec<String>,
}

impl CliValues {
    pub fn contains(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.values
            .get(name)
            .and_then(|values| values.last())
            .and_then(Option::as_deref)
    }

    pub fn get_all(&self, name: &str) -> impl Iterator<Item = Option<&str>> {
        self.values
            .get(name)
            .into_iter()
            .flatten()
            .map(Option::as_deref)
    }

    pub fn positionals(&self) -> &[String] {
        &self.positionals
    }
}

pub enum CliParseOutcome {
    Help,
    Parsed(CliValues),
}

pub fn parse_cli(
    args: impl IntoIterator<Item = String>,
    program: &str,
    specs: &'static [CliOptionSpec],
) -> Result<CliParseOutcome, CommandError> {
    let args = args.into_iter().collect::<Vec<_>>();
    let usage = format_cli_help(program, specs);
    let mut parsed = CliValues {
        values: BTreeMap::new(),
        positionals: Vec::new(),
    };
    let mut index = 0;
    let mut options_enabled = true;
    while index < args.len() {
        let argument = &args[index];
        if options_enabled && argument == "--" {
            options_enabled = false;
            index += 1;
            continue;
        }
        if !options_enabled || !argument.starts_with('-') {
            parsed.positionals.push(argument.clone());
            index += 1;
            continue;
        }

        let (spelling, inline_value) = argument
            .split_once('=')
            .map_or((argument.as_str(), None), |(name, value)| {
                (name, Some(value))
            });
        let spec = specs
            .iter()
            .find(|spec| spec.name == spelling || spec.alias == Some(spelling))
            .ok_or_else(|| {
                CommandError::new(
                    CommandErrorCode::UnknownOption,
                    Some(spelling),
                    format!("unknown option `{spelling}`"),
                    Some(usage.as_str()),
                )
            })?;
        let value = match (spec.value_name, inline_value) {
            (None, Some(_)) => {
                return Err(CommandError::new(
                    CommandErrorCode::InvalidArguments,
                    Some(spelling),
                    "option does not take a value",
                    Some(usage.as_str()),
                ));
            }
            (None, None) => None,
            (Some(_), Some(value)) => Some(value.to_owned()),
            (Some(_), None) => {
                let next = args.get(index + 1).map(String::as_str);
                if next.is_none_or(|next| {
                    next == "--"
                        || next.starts_with("--")
                        || specs.iter().any(|candidate| candidate.alias == Some(next))
                }) {
                    return Err(CommandError::new(
                        CommandErrorCode::MissingValue,
                        Some(spec.name),
                        format!("{} requires {}", spec.name, spec.value_name.unwrap()),
                        Some(usage.as_str()),
                    ));
                }
                index += 1;
                Some(args[index].clone())
            }
        };
        let entry = parsed.values.entry(spec.name).or_default();
        if !spec.repeatable && !entry.is_empty() {
            return Err(CommandError::new(
                CommandErrorCode::DuplicateOption,
                Some(spec.name),
                format!("{} may only be specified once", spec.name),
                Some(usage.as_str()),
            ));
        }
        entry.push(value);
        index += 1;
    }
    if specs
        .iter()
        .any(|spec| spec.help && parsed.values.contains_key(spec.name))
    {
        Ok(CliParseOutcome::Help)
    } else {
        Ok(CliParseOutcome::Parsed(parsed))
    }
}

pub fn format_cli_help(program: &str, specs: &'static [CliOptionSpec]) -> String {
    let options = specs
        .iter()
        .map(|spec| {
            let alias = spec
                .alias
                .map_or(String::new(), |alias| format!(", {alias}"));
            let value = spec
                .value_name
                .map_or(String::new(), |name| format!(" {name}"));
            format!("  {}{alias}{value}\n      {}", spec.name, spec.description)
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{program}\n{options}")
}
