use crate::{CommandError, CommandErrorCode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandToken {
    pub value: String,
    pub span: SourceSpan,
}

/// Splits a command line into words while preserving whitespace inside quotes.
pub fn lex_command_line(line: &str) -> Result<Vec<String>, CommandError> {
    Ok(lex_command_line_with_spans(line)?
        .into_iter()
        .map(|token| token.value)
        .collect())
}

/// Tokenizes command text while retaining source byte ranges for raw-tail consumers.
/// Single quotes are literal; double quotes only escape `\\` and `\"`.
pub fn lex_command_line_with_spans(line: &str) -> Result<Vec<CommandToken>, CommandError> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    let mut start = 0;
    let mut index = 0;
    let mut characters = line.char_indices().peekable();
    while let Some((offset, character)) = characters.next() {
        index = offset + character.len_utf8();
        if character.is_control() {
            return Err(syntax_error("command contains a control character"));
        }
        if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            } else if delimiter == '"' && character == '\\' {
                match characters.peek().copied() {
                    Some((_, escaped @ ('\\' | '"'))) => {
                        characters.next();
                        index += escaped.len_utf8();
                        word.push(escaped);
                    }
                    _ => word.push(character),
                }
            } else {
                word.push(character);
            }
        } else if matches!(character, '\'' | '"') {
            quote = Some(character);
            if !started {
                start = offset;
            }
            started = true;
        } else if character.is_whitespace() {
            if started {
                words.push(CommandToken {
                    value: std::mem::take(&mut word),
                    span: SourceSpan { start, end: offset },
                });
                started = false;
            }
        } else {
            if !started {
                start = offset;
            }
            word.push(character);
            started = true;
        }
    }
    if quote.is_some() {
        return Err(syntax_error("command has an unclosed quoted value"));
    }
    if started {
        words.push(CommandToken {
            value: word,
            span: SourceSpan { start, end: index },
        });
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
