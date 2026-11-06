//! Generic command metadata registry shared by domain-owned command declarations.

mod cli;
mod config;
mod error;
mod property;
mod registry;
mod syntax;

pub use cli::{format_cli_help, parse_cli, CliOptionSpec, CliParseOutcome, CliValues};
pub use config::{
    ConfigDocument, ConfigError, ConfigErrorCode, ConfigResolution, ResolvedConfigValue,
    MAX_CONFIG_BYTES,
};
pub use error::{CommandError, CommandErrorCode};
pub use property::{
    parse_property_request, PropertyLifecycle, PropertyPermission, PropertyRegistry,
    PropertyRequest, PropertySource, PropertySpec, PropertyType, PropertyValue,
};
pub use registry::{
    format_command_help, CommandArgumentSpec, CommandArgumentType, CommandOutcome, CommandRegistry,
    CommandRegistryError, CommandSpec,
};
pub use syntax::{
    format_command_arguments, lex_command_line, lex_command_line_with_spans, CommandToken,
    SourceSpan,
};
