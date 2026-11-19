//! Generic command metadata registry shared by domain-owned command declarations.

pub mod admin;
mod cli;
pub mod completion;
mod config;
mod error;
pub mod operation;
mod property;
mod registry;
pub mod results;
pub mod syntax;

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
    format_command_help, write_command_help, CommandArgumentSpec, CommandArgumentType,
    CommandOutcome, CommandRegistry, CommandRegistryError, CommandSpec,
};
pub use syntax::{
    format_command_arguments, lex_command_line, lex_command_line_with_spans, CommandToken,
    SourceSpan,
};

/// Public client command-line budget, before trimming or decoding values.
pub const MAX_INPUT_BYTES: usize = 512;
pub fn input_too_long(bytes: usize) -> CommandError {
    CommandError::new(
        CommandErrorCode::InputTooLong,
        Some("input"),
        format!("input exceeds {MAX_INPUT_BYTES} bytes ({bytes} received)"),
        None::<String>,
    )
    .at(SourceSpan {
        start: 0,
        end: bytes,
    })
}
