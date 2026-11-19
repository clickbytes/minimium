//! Prepare an operation using catalog metadata only. The owner validates domain values.
use super::canonical::{diagnostic, parse_statement, AccessKind, Literal, Statement};
use crate::operation::{
    Access, CatalogEntry, CommandValue, OperationCatalog, OperationId, OperationKind, TypeRef,
};
use crate::{
    lex_command_line_with_spans, CommandError, CommandErrorCode as Code, PropertyLifecycle,
    SourceSpan,
};

#[derive(Clone, Debug, PartialEq)]
pub struct BoundArgument {
    pub schema: TypeRef,
    pub value: CommandValue,
    pub text: String,
    pub span: SourceSpan,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedOperation {
    pub id: OperationId,
    pub path: String,
    pub kind: AccessKind,
    pub access: Access,
    pub arguments: Vec<BoundArgument>,
    pub spelling: String,
    pub span: SourceSpan,
    pub legacy: bool,
}

impl PreparedOperation {
    pub fn property_request(&self, entry: &CatalogEntry) -> Option<crate::PropertyRequest> {
        let key = entry.property?.key.to_owned();
        match self.kind {
            AccessKind::Read => Some(crate::PropertyRequest::Get { key }),
            AccessKind::Reset => Some(crate::PropertyRequest::Reset { key }),
            AccessKind::Write => Some(crate::PropertyRequest::Set {
                key,
                value: self.arguments.first()?.text.clone(),
            }),
            AccessKind::Invoke => None,
        }
    }

    pub fn explanation(&self, entry: &CatalogEntry) -> OperationExplanation {
        OperationExplanation {
            id: self.id.clone(),
            path: self.path.clone(),
            access: self.access,
            kind: self.kind,
            arguments: self.arguments.clone(),
            bindings: entry.bindings.iter().copied().collect(),
            legacy_spelling: self.legacy.then(|| self.spelling.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OperationExplanation {
    pub id: OperationId,
    pub path: String,
    pub access: Access,
    pub kind: AccessKind,
    pub arguments: Vec<BoundArgument>,
    pub bindings: Vec<&'static str>,
    pub legacy_spelling: Option<String>,
}

impl std::fmt::Display for OperationExplanation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let values = self
            .arguments
            .iter()
            .map(|a| format!("{}={:?}", a.schema.0, a.value))
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "operation={} path={} access={:?} arguments=[{}] effect={:?} permission={} source={:?} legacy={:?}",
            self.id.0, self.path, self.kind, values, self.access.effect, self.access.permission,
            self.bindings, self.legacy_spelling)
    }
}

pub enum ParsedLine {
    Operation(PreparedOperation),
    Namespace(String),
    /// A host grammar such as list, help with a group, or an unregistered extension.
    Legacy,
}

/// Match a registered word command without interpreting punctuation in its remaining text.
pub fn legacy_prefix<'a>(
    catalog: &'a OperationCatalog,
    line: &str,
) -> Option<(&'a CatalogEntry, &'static str, SourceSpan)> {
    let mut words = line.split_whitespace();
    let mut path = String::new();
    let mut best = None;
    let mut scan = 0;
    for _ in 0..16 {
        let Some(word) = words.next() else {
            break;
        };
        if !word
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
        {
            break;
        }
        let offset = scan + line[scan..].find(word)?;
        scan = offset + word.len();
        if !path.is_empty() {
            path.push(' ');
        }
        path.push_str(word);
        if let Some(entry) = catalog.resolve(&path) {
            if let Some(alias) = entry.aliases.iter().find(|a| a.name == path) {
                best = Some((
                    entry,
                    alias.name,
                    SourceSpan {
                        start: line.len() - line.trim_start().len(),
                        end: scan,
                    },
                ));
            }
        }
    }
    best
}

pub fn bind_line(catalog: &OperationCatalog, line: &str) -> Result<ParsedLine, CommandError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(ParsedLine::Legacy);
    }
    let first = trimmed.split_whitespace().next().unwrap_or_default();
    if matches!(first, "get" | "set" | "reset") {
        let words = lex_command_line_with_spans(line)?;
        let Some(key) = words.get(1) else {
            return Err(diagnostic(
                Code::MissingArgument,
                first,
                "expected property path",
                SourceSpan {
                    start: line.len(),
                    end: line.len(),
                },
            ));
        };
        let entry = catalog.resolve(&key.value).ok_or_else(|| {
            diagnostic(
                Code::UnknownProperty,
                &key.value,
                "unknown property",
                key.span,
            )
        })?;
        let kind = match first {
            "set" => AccessKind::Write,
            "reset" => AccessKind::Reset,
            _ => AccessKind::Read,
        };
        let arguments = words[2..]
            .iter()
            .map(|w| Literal {
                text: w.value.clone(),
                quoted: false,
                span: w.span,
            })
            .collect();
        return prepare(
            entry,
            Statement {
                name: format!("{first} {}", key.value),
                span: key.span,
                access: kind,
                arguments,
            },
            true,
        )
        .map(ParsedLine::Operation);
    }
    if first == "list" {
        return Ok(ParsedLine::Legacy);
    }
    if let Some((entry, name, span)) = legacy_prefix(catalog, line) {
        let tail = &line[span.end..];
        let name_spec = entry
            .aliases
            .iter()
            .find(|a| a.name == name)
            .expect("resolved alias");
        let rest = entry.legacy_option.is_none()
            && name_spec
                .arguments
                .last()
                .is_some_and(|arg| arg.rest && name_spec.arguments.len() == 1);
        let tokens = lex_command_line_with_spans(tail).map_err(|mut err| {
            if let Some(range) = &mut err.span {
                range.start += span.end;
                range.end += span.end;
            }
            err
        })?;
        let mut arguments = if name == "help" && !tokens.is_empty() {
            vec![Literal {
                span: SourceSpan {
                    start: span.end + tokens[0].span.start,
                    end: span.end + tokens.last().unwrap().span.end,
                },
                text: tokens
                    .iter()
                    .map(|w| w.value.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
                quoted: false,
            }]
        } else if rest && !tail.trim().is_empty() {
            let t = tail.trim_start();
            vec![Literal {
                text: t.to_owned(),
                quoted: false,
                span: SourceSpan {
                    start: line.len() - t.len(),
                    end: line.len(),
                },
            }]
        } else {
            tokens
                .into_iter()
                .map(|w| Literal {
                    text: w.value,
                    quoted: false,
                    span: SourceSpan {
                        start: span.end + w.span.start,
                        end: span.end + w.span.end,
                    },
                })
                .collect()
        };
        // Legacy commands can encode a fixed enum in their registered name.
        if let Some(fixed) = entry.fixed_arguments.get(name) {
            arguments.insert(
                0,
                Literal {
                    text: (*fixed).into(),
                    quoted: false,
                    span,
                },
            );
        }
        if let Some(flag) = entry.legacy_option {
            if arguments.first().is_some_and(|arg| arg.text == flag) {
                arguments.remove(0);
                if arguments.is_empty() {
                    return Err(diagnostic(
                        Code::MissingArgument,
                        name,
                        "option requires an argument",
                        span,
                    ));
                }
            } else if !arguments.is_empty() {
                return Err(diagnostic(
                    Code::InvalidArguments,
                    name,
                    "expected option before argument",
                    span,
                ));
            }
        }
        if arguments.is_empty() && name_spec.arguments.first().is_some_and(|arg| !arg.optional) {
            return Err(CommandError::new(
                Code::MissingArgument,
                Some(name),
                "expected argument",
                Some(name_spec.usage),
            )
            .at(span));
        }
        let access = match entry.spec.kind {
            OperationKind::Method => AccessKind::Invoke,
            OperationKind::Property if arguments.is_empty() => AccessKind::Read,
            OperationKind::Property => AccessKind::Write,
        };
        return prepare(
            entry,
            Statement {
                name: name.into(),
                span,
                access,
                arguments,
            },
            true,
        )
        .map(ParsedLine::Operation);
    }
    if !first.contains(['.', '(', '=']) && !trimmed.contains(['(', '=']) {
        if !catalog.children(trimmed).is_empty() {
            return Ok(ParsedLine::Namespace(trimmed.into()));
        }
        return Ok(ParsedLine::Legacy);
    }
    let statement = parse_statement(line)?;
    if let Some(entry) = catalog.resolve(&statement.name) {
        return prepare(entry, statement, false).map(ParsedLine::Operation);
    }
    if statement.access == AccessKind::Read && !catalog.children(&statement.name).is_empty() {
        return Ok(ParsedLine::Namespace(statement.name));
    }
    Err(diagnostic(
        Code::UnknownCommand,
        &statement.name,
        "unknown operation",
        statement.span,
    ))
}

fn prepare(
    entry: &CatalogEntry,
    s: Statement,
    legacy: bool,
) -> Result<PreparedOperation, CommandError> {
    let spec = &entry.spec;
    let fail = |code, reason| diagnostic(code, &s.name, reason, s.span);
    let access = match (spec.kind, s.access) {
        (OperationKind::Property, AccessKind::Invoke) => {
            return Err(fail(
                Code::NotCallable,
                "property requires a read or assignment",
            ))
        }
        (OperationKind::Method, AccessKind::Read) => {
            return Err(fail(Code::MissingArgument, "method requires parentheses"))
        }
        (OperationKind::Method, AccessKind::Write | AccessKind::Reset) => {
            return Err(fail(Code::NotAssignable, "method cannot be assigned"))
        }
        (_, AccessKind::Invoke) => spec.invoke,
        (_, AccessKind::Read) => spec.read,
        (_, AccessKind::Write | AccessKind::Reset) => {
            match spec.lifecycle {
                PropertyLifecycle::CreationOnly => {
                    return Err(fail(Code::CreationOnly, "setting is fixed at creation"))
                }
                PropertyLifecycle::RestartRequired => {
                    return Err(fail(Code::RestartRequired, "setting requires restart"))
                }
                PropertyLifecycle::ReadOnly => {
                    return Err(fail(Code::ReadOnly, "property is read-only"))
                }
                PropertyLifecycle::Live => {}
            }
            if s.access == AccessKind::Reset && !entry.property.is_some_and(|p| p.resettable) {
                return Err(fail(Code::InvalidArguments, "reset is unavailable"));
            }
            spec.write
        }
    }
    .ok_or_else(|| fail(Code::UnsupportedContext, "requested access is unavailable"))?;
    let params = if matches!(s.access, AccessKind::Read | AccessKind::Reset) {
        &[][..]
    } else {
        spec.arguments.as_slice()
    };
    if let Some(extra) = s.arguments.get(params.len()) {
        return Err(diagnostic(
            Code::InvalidArguments,
            &s.name,
            "unexpected argument",
            extra.span,
        ));
    }
    let mut arguments = Vec::new();
    for (index, p) in params.iter().enumerate() {
        let Some(token) = s.arguments.get(index) else {
            if p.optional {
                continue;
            }
            return Err(diagnostic(
                Code::MissingArgument,
                p.name,
                &format!("expected {}", p.value_type.0),
                s.span,
            ));
        };
        let mut value = token.text.clone();
        let typed = if p.optional && value == "none" && !token.quoted {
            CommandValue::Unit
        } else {
            super::values::bind_value(p.scalar, &mut value, token.quoted, legacy).ok_or_else(
                || {
                    diagnostic(
                        Code::InvalidValue,
                        p.name,
                        &format!("expected {}", p.value_type.0),
                        token.span,
                    )
                },
            )?
        };
        arguments.push(BoundArgument {
            schema: p.value_type,
            value: typed,
            text: value,
            span: token.span,
        });
    }
    Ok(PreparedOperation {
        id: spec.id.clone(),
        path: spec.path.clone(),
        kind: s.access,
        access,
        arguments,
        spelling: s.name,
        span: s.span,
        legacy,
    })
}

pub fn explain_entry() -> CatalogEntry {
    use crate::operation::{DomainBinding, Effect, ParameterSpec, PersistenceKind, ScalarType};
    DomainBinding {
        owner: "OperationCatalog",
        scope: "Host",
        context: "visible catalog",
        permission: "Local",
        capability: None,
        source: "Catalog",
        persistence: PersistenceKind::None,
    }
    .declaration(
        "ui.console.explain",
        OperationKind::Method,
        &[ParameterSpec::required("text", "String").scalar(ScalarType::Text)],
        TypeRef("OperationExplanation"),
        None,
        Effect::Query,
    )
}
