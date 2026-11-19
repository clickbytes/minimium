use super::{
    ranges, signature::signature_help, CompletionItem, CompletionResult, ItemKind, MAX_ITEMS,
};
use crate::operation::{CatalogEntry, OperationCatalog, OperationKind, TypeRef};
use crate::SourceSpan;
use std::ops::Range;

impl CompletionResult {
    fn offer(
        &mut self,
        label: &str,
        insert: String,
        caret: usize,
        range: &Range<usize>,
        kind: ItemKind,
        entry: Option<&CatalogEntry>,
        ty: Option<TypeRef>,
    ) {
        let index = match self
            .items
            .binary_search_by(|item| item.label.as_str().cmp(label))
        {
            Ok(_) => return,
            Err(i) => i,
        };
        if self.items.len() == MAX_ITEMS {
            self.truncated = true;
            if index >= MAX_ITEMS {
                return;
            }
            self.items.pop();
        }
        let spec = entry.map(|e| &e.spec);
        self.items.insert(
            index,
            CompletionItem {
                label: label.into(),
                insert_text: insert,
                caret_offset: caret,
                replace_range: range.clone(),
                buffer_revision: self.buffer_revision,
                kind,
                value_type: ty.or_else(|| spec.map(|s| s.result)),
                unit: spec.and_then(|s| s.unit),
                access: spec.and_then(|s| match kind {
                    ItemKind::Argument => s.invoke.or(s.write),
                    _ => s.invoke.or(s.read),
                }),
                description: spec
                    .map_or_else(String::new, |s| s.help_key.chars().take(160).collect()),
                operation: spec.map(|s| s.id.clone()),
            },
        );
    }
}

pub(super) fn populate(
    out: &mut CompletionResult,
    line: &str,
    cursor: usize,
    selection: Range<usize>,
    catalog: &OperationCatalog,
    explicit: bool,
    canonical: bool,
    alias: Option<(&CatalogEntry, &'static str, SourceSpan)>,
) {
    let first = out.tokens.first();
    let verb = first.map_or("", |t| &line[t.span.start..t.span.end]);
    let head_end = first.map_or(0, |t| t.span.end);
    let before = out
        .tokens
        .iter()
        .take_while(|t| t.span.end < cursor)
        .count();
    let word = ranges::word(&out.tokens, cursor);
    let mut argument = None;
    if canonical && cursor > head_end {
        if let Some(entry) = catalog.resolve(verb) {
            let mut active = None;
            for t in out
                .tokens
                .iter()
                .skip(1)
                .take_while(|t| t.span.end <= cursor)
            {
                match t.value.as_str() {
                    "(" | "=" => active = Some(0),
                    "," => active = active.map(|a| a + 1),
                    ")" => active = None,
                    _ => {}
                }
            }
            if let Some(active) = active {
                argument = Some((entry, active, None));
            }
        }
        if argument.is_none() {
            return;
        }
    } else if explicit && before >= 2 {
        if verb == "set" && before == 2 {
            argument = out
                .tokens
                .get(1)
                .and_then(|t| catalog.resolve(&t.value))
                .map(|e| (e, 0, None));
        }
        if argument.is_none() {
            return;
        }
    } else if !explicit {
        if let Some((entry, name, span)) = alias.filter(|(_, _, s)| cursor > s.end) {
            let desc = entry.aliases.iter().find(|a| a.name == name);
            let mut active = out
                .tokens
                .iter()
                .filter(|t| t.span.start >= span.end && t.span.end < cursor)
                .count();
            if let Some(rest) = desc.and_then(|a| a.arguments.iter().position(|p| p.rest)) {
                active = active.min(rest);
            }
            argument = Some((entry, active, desc));
        }
    }
    if let Some((entry, active, alias)) = argument {
        let typed_index =
            active + usize::from(alias.is_some_and(|a| entry.fixed_arguments.contains_key(a.name)));
        out.signature = Some(signature_help(entry, typed_index));
        let word = if canonical && matches!(&line[word.clone()], "(" | "," | "=" | ")") {
            cursor..cursor
        } else {
            word
        };
        let range = ranges::replacement_range(line, cursor, &selection, word.clone(), false);
        let prefix = line[word.start..cursor].trim_start_matches('"');
        let ty = entry.spec.arguments.get(typed_index).map(|a| a.value_type);
        let choices = alias
            .and_then(|a| a.arguments.get(active))
            .and_then(|a| a.choices);
        let property = entry.property.and_then(|p| {
            matches!(
                p.value_type,
                crate::PropertyType::Boolean | crate::PropertyType::Enumeration
            )
            .then_some(p.constraints)
            .flatten()
        });
        let choices = choices
            .into_iter()
            .flatten()
            .copied()
            .chain(property.into_iter().flat_map(|p| p.split('|')));
        for value in choices.filter(|v| v.starts_with(prefix)) {
            out.offer(
                value,
                value.into(),
                value.len(),
                &range,
                ItemKind::Argument,
                Some(entry),
                ty,
            );
        }
        return;
    }
    let property = explicit && cursor > head_end;
    let token_start = word.start;
    let range = ranges::replacement_range(line, cursor, &selection, word, true);
    let focus = cursor.clamp(range.start, range.end);
    let prefix = &line[range.start..focus];
    let scope = line.get(token_start..range.start).unwrap_or("");
    let preceding = line[..token_start].trim();
    let suffix = &line[range.end..];
    let parent = if property || preceding.is_empty() {
        String::new()
    } else {
        format!("{preceding} ")
    };
    let search = format!("{parent}{scope}{prefix}");
    for (name, entry) in catalog.spellings(&search) {
        if property
            && matches!(verb, "get" | "set" | "reset")
            && (entry.spec.kind != OperationKind::Property
                || verb == "set" && entry.spec.write.is_none()
                || verb == "reset" && !entry.property.is_some_and(|p| p.resettable))
        {
            continue;
        }
        let is_alias = name != entry.spec.path;
        let tail = &name[parent.len() + scope.len()..];
        let end = tail.find(['.', ' ']).unwrap_or(tail.len());
        let label = &tail[..end];
        if label.is_empty() || !label.starts_with(prefix) {
            continue;
        }
        let mut insert = label.to_owned();
        let namespace = end < tail.len();
        let kind = if namespace {
            ItemKind::Namespace
        } else if entry.spec.kind == OperationKind::Property {
            ItemKind::Property
        } else {
            ItemKind::Method
        };
        let mut caret = insert.len();
        if namespace {
            let sep = tail.as_bytes()[end] as char;
            if !suffix.starts_with(sep) {
                insert.push(sep);
            }
            caret += 1;
        } else if kind == ItemKind::Method && !is_alias && !property {
            let spaces = suffix.len() - suffix.trim_start().len();
            if suffix.trim_start().starts_with('(') {
                caret += spaces + 1;
            } else {
                insert.push_str("()");
                caret += if entry.spec.arguments.is_empty() {
                    2
                } else {
                    1
                };
            }
        }
        out.offer(
            label,
            insert,
            caret,
            &range,
            kind,
            (!namespace).then_some(entry),
            None,
        );
    }
}
