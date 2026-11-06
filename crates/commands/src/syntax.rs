use crate::{CommandError, CommandErrorCode};

/// Splits a command line into words while preserving whitespace inside quotes.
pub fn lex_command_line(line: &str) -> Result<Vec<String>, CommandError> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut started = false;
    for character in line.chars() {
        if character.is_control() {
            return Err(syntax_error("command contains a control character"));
        }
        if escaped {
            word.push(character);
            escaped = false;
            started = true;
        } else if quoted && character == '\\' {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
            started = true;
        } else if character.is_whitespace() && !quoted {
            if started {
                words.push(std::mem::take(&mut word));
                started = false;
            }
        } else {
            word.push(character);
            started = true;
        }
    }
    if escaped || quoted {
        return Err(syntax_error("command has an unclosed quoted value"));
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

/// Converts command arguments to the bounded text line used by local admin IPC.
/// A single positional may already be the user's complete raw command line.
pub fn format_command_arguments(arguments: &[String]) -> Result<String, CommandError> {
    if arguments.is_empty() {
        return Err(syntax_error("a server command is required"));
    }
    let line = if arguments.len() == 1 && arguments[0].chars().any(char::is_whitespace) {
        let raw = arguments[0].trim().to_owned();
        lex_command_line(&raw)?;
        raw
    } else {
        arguments
            .iter()
            .map(|argument| {
                if argument.is_empty()
                    || argument.chars().any(|character| {
                        character.is_whitespace() || matches!(character, '"' | '\\')
                    })
                {
                    format!(
                        "\"{}\"",
                        argument.replace('\\', "\\\\").replace('"', "\\\"")
                    )
                } else {
                    argument.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    if line.is_empty() || line.len() > 512 {
        return Err(syntax_error("command must contain 1..512 bytes"));
    }
    lex_command_line(&line)?;
    Ok(line)
}

fn syntax_error(reason: &str) -> CommandError {
    CommandError::new(
        CommandErrorCode::InvalidArguments,
        Some("command"),
        reason,
        Some("<server command>"),
    )
}
