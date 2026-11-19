use crate::{CommandError, CommandErrorCode as Code, SourceSpan};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Literal {
    pub text: String,
    pub quoted: bool,
    pub span: SourceSpan,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccessKind {
    Read,
    Write,
    Reset,
    Invoke,
}

#[derive(Clone, Debug)]
pub struct Statement {
    pub name: String,
    pub span: SourceSpan,
    pub access: AccessKind,
    pub arguments: Vec<Literal>,
}

pub(super) fn diagnostic(
    code: Code,
    subject: &str,
    reason: &str,
    span: SourceSpan,
) -> CommandError {
    CommandError::new(
        code,
        Some(subject),
        reason,
        Some("path | path = literal | path(arguments)"),
    )
    .at(span)
}

struct Cursor<'a> {
    line: &'a str,
    at: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<char> {
        self.line[self.at..].chars().next()
    }
    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += c.len_utf8();
        Some(c)
    }
    fn spaces(&mut self) {
        while self.peek().is_some_and(|c| c == ' ' || c == '\t') {
            self.next();
        }
    }
    fn span(&self, start: usize) -> SourceSpan {
        SourceSpan {
            start,
            end: self.at,
        }
    }
    fn error(&self, reason: &str) -> CommandError {
        let end = self.at + self.peek().map_or(0, char::len_utf8);
        diagnostic(
            Code::InvalidArguments,
            "syntax",
            reason,
            SourceSpan {
                start: self.at,
                end,
            },
        )
    }
    fn literal(&mut self) -> Result<Literal, CommandError> {
        self.spaces();
        let start = self.at;
        let quoted = self.peek() == Some('"');
        let text = if quoted {
            self.next();
            let mut out = String::new();
            loop {
                let c = self
                    .next()
                    .ok_or_else(|| self.error("unterminated string"))?;
                match c {
                    '"' => break,
                    '\\' => out.push(self.escape()?),
                    c if c.is_control() => return Err(self.error("unescaped control character")),
                    c => out.push(c),
                }
            }
            out
        } else {
            while self
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || "_.+-".contains(c))
            {
                self.next();
            }
            if start == self.at {
                return Err(self.error("expected literal"));
            }
            self.line[start..self.at].to_owned()
        };
        Ok(Literal {
            text,
            quoted,
            span: self.span(start),
        })
    }
    fn escape(&mut self) -> Result<char, CommandError> {
        Ok(match self.next() {
            Some(c @ ('"' | '\\')) => c,
            Some('n') => '\n',
            Some('r') => '\r',
            Some('t') => '\t',
            Some('u') if self.next() == Some('{') => {
                let start = self.at;
                while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                    self.next();
                }
                let digits = &self.line[start..self.at];
                if digits.len() > 6 || self.next() != Some('}') {
                    return Err(self.error("invalid Unicode escape"));
                }
                u32::from_str_radix(digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| self.error("invalid Unicode scalar"))?
            }
            _ => return Err(self.error("unknown string escape")),
        })
    }
}

/// A valid prefix can be retained for editing without turning it into an invocation.
#[derive(Clone, Debug)]
pub enum PartialStatement {
    Complete(Statement),
    Incomplete(CommandError),
    Invalid(CommandError),
}

pub fn parse_partial_statement(line: &str) -> PartialStatement {
    match parse_statement(line) {
        Ok(statement) => PartialStatement::Complete(statement),
        Err(error) if error.span.is_some_and(|s| s.start == line.len()) => {
            PartialStatement::Incomplete(error)
        }
        Err(error) => PartialStatement::Invalid(error),
    }
}

/// Parse a single public expression. Byte ranges always end on UTF-8 boundaries.
pub fn parse_statement(line: &str) -> Result<Statement, CommandError> {
    let mut cur = Cursor { line, at: 0 };
    cur.spaces();
    let start = cur.at;
    while cur
        .peek()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c))
    {
        cur.next();
    }
    let name = &line[start..cur.at];
    if name.split('.').any(str::is_empty) {
        return Err(cur.error("expected complete operation path"));
    }
    let span = cur.span(start);
    cur.spaces();
    let mut arguments = Vec::new();
    let access = match cur.next() {
        None => AccessKind::Read,
        Some('=') => {
            arguments.push(cur.literal()?);
            AccessKind::Write
        }
        Some('(') => {
            cur.spaces();
            if cur.peek() != Some(')') {
                loop {
                    arguments.push(cur.literal()?);
                    cur.spaces();
                    if cur.peek() != Some(',') {
                        break;
                    }
                    cur.next();
                }
            }
            if cur.next() != Some(')') {
                return Err(cur.error("expected closing parenthesis"));
            }
            AccessKind::Invoke
        }
        _ => return Err(cur.error("expected assignment or method call")),
    };
    cur.spaces();
    if cur.peek().is_some() {
        return Err(cur.error("unexpected trailing input"));
    }
    Ok(Statement {
        name: name.into(),
        span,
        access,
        arguments,
    })
}
