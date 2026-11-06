use crate::{lex_command_line, CommandError, CommandErrorCode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropertyType {
    Boolean,
    Integer,
    Decimal,
    Text,
    Enumeration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropertyPermission {
    Local,
    Player,
    WorldAdmin,
    ServerAdmin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropertyLifecycle {
    Live,
    RestartRequired,
    CreationOnly,
    ReadOnly,
}

/// Read-only metadata for one property whose state remains owned by its domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropertySpec {
    pub key: &'static str,
    pub value_type: PropertyType,
    pub unit: Option<&'static str>,
    pub description: &'static str,
    pub default: Option<&'static str>,
    pub constraints: Option<&'static str>,
    pub owner: &'static str,
    pub permission: PropertyPermission,
    pub lifecycle: PropertyLifecycle,
    pub persistent: bool,
    pub resettable: bool,
}

impl PropertySpec {
    pub const fn new(
        key: &'static str,
        value_type: PropertyType,
        unit: Option<&'static str>,
        description: &'static str,
        default: Option<&'static str>,
        constraints: Option<&'static str>,
        owner: &'static str,
        permission: PropertyPermission,
        lifecycle: PropertyLifecycle,
        persistent: bool,
        resettable: bool,
    ) -> Self {
        Self {
            key,
            value_type,
            unit,
            description,
            default,
            constraints,
            owner,
            permission,
            lifecycle,
            persistent,
            resettable,
        }
    }
}

#[derive(Default)]
pub struct PropertyRegistry {
    properties: Vec<&'static PropertySpec>,
}

impl PropertyRegistry {
    pub fn register(&mut self, specs: &'static [PropertySpec]) -> Result<(), CommandError> {
        let mut staged = Vec::with_capacity(specs.len());
        for spec in specs {
            if !valid_property_key(spec.key) {
                return Err(CommandError::new(
                    CommandErrorCode::InvalidValue,
                    Some(spec.key),
                    "invalid property key",
                    None::<String>,
                ));
            }
            if self
                .properties
                .iter()
                .any(|existing| existing.key == spec.key)
                || staged
                    .iter()
                    .any(|existing: &&PropertySpec| existing.key == spec.key)
            {
                return Err(CommandError::new(
                    CommandErrorCode::InvalidValue,
                    Some(spec.key),
                    "duplicate property key",
                    None::<String>,
                ));
            }
            staged.push(spec);
        }
        self.properties.extend(staged);
        self.properties.sort_by_key(|spec| spec.key);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&'static PropertySpec> {
        self.properties.iter().copied().find(|spec| spec.key == key)
    }

    pub fn list(&self, prefix: Option<&str>) -> impl Iterator<Item = &'static PropertySpec> + '_ {
        let prefix = prefix.map(str::to_owned);
        self.properties.iter().copied().filter(move |spec| {
            prefix
                .as_deref()
                .is_none_or(|prefix| spec.key.starts_with(prefix))
        })
    }

    pub fn specs(&self) -> impl Iterator<Item = &'static PropertySpec> + '_ {
        self.properties.iter().copied()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PropertyRequest {
    Get { key: String },
    Set { key: String, value: String },
    Reset { key: String },
    List { prefix: Option<String> },
    Help { target: Option<String> },
}

pub fn parse_property_request(input: &str) -> Result<Option<PropertyRequest>, CommandError> {
    let words = lex_command_line(input)?;
    let Some(operation) = words.first().map(String::as_str) else {
        return Ok(None);
    };
    let usage = "get KEY | set KEY VALUE | reset KEY | list [PREFIX] | help [COMMAND|KEY]";
    let request = match operation {
        "get" | "reset" => {
            let key = words.get(1).ok_or_else(|| {
                CommandError::new(
                    CommandErrorCode::MissingValue,
                    Some("KEY"),
                    format!("{operation} requires a property key"),
                    Some(usage),
                )
            })?;
            if words.len() > 2 {
                return Err(invalid_property_arguments(operation, usage));
            }
            let key = key.to_owned();
            if operation == "get" {
                PropertyRequest::Get { key }
            } else {
                PropertyRequest::Reset { key }
            }
        }
        "set" => {
            let key = words.get(1).ok_or_else(|| {
                CommandError::new(
                    CommandErrorCode::MissingValue,
                    Some("KEY"),
                    "set requires a property key",
                    Some(usage),
                )
            })?;
            let value = words.get(2).ok_or_else(|| {
                CommandError::new(
                    CommandErrorCode::MissingValue,
                    Some(key),
                    "set requires a value",
                    Some(usage),
                )
            })?;
            if words.len() > 3 {
                return Err(invalid_property_arguments(operation, usage));
            }
            PropertyRequest::Set {
                key: key.to_owned(),
                value: value.to_owned(),
            }
        }
        "list" => {
            let prefix = words.get(1).cloned();
            if words.len() > 2 {
                return Err(invalid_property_arguments(operation, usage));
            }
            PropertyRequest::List { prefix }
        }
        "help" => {
            let target = (words.len() > 1)
                .then(|| words.iter().skip(1).cloned().collect::<Vec<_>>().join(" "));
            PropertyRequest::Help { target }
        }
        _ => return Ok(None),
    };
    Ok(Some(request))
}

fn invalid_property_arguments(operation: &str, usage: &str) -> CommandError {
    CommandError::new(
        CommandErrorCode::InvalidArguments,
        Some(operation),
        "unexpected or missing arguments",
        Some(usage),
    )
}

fn valid_property_key(key: &str) -> bool {
    let mut components = key.split('.');
    components.all(|component| {
        !component.is_empty()
            && component.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte)
            })
    }) && key.contains('.')
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropertySource {
    Default,
    File,
    Environment,
    CommandLine,
    Session,
    Runtime,
    Replica,
    Unavailable,
}

impl PropertySource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::File => "file",
            Self::Environment => "environment",
            Self::CommandLine => "command_line",
            Self::Session => "session",
            Self::Runtime => "runtime",
            Self::Replica => "replica",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyValue<T> {
    pub requested: Option<T>,
    pub effective: Option<T>,
    pub default: Option<T>,
    pub source: PropertySource,
    pub difference_reason: Option<String>,
}
