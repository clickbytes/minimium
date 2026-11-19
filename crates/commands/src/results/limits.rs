use super::{OperationResult, Payload};
use crate::operation::CommandValue;
use std::fmt::{self, Write};

pub const MAX_PAYLOAD_BYTES: usize = 32 * 1024;
pub const MAX_ADMIN_BYTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 8;
const MAX_ELEMENTS: usize = 1024;

struct Budget(usize);
impl Write for Budget {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.0 = self.0.checked_sub(text.len()).ok_or(fmt::Error)?;
        Ok(())
    }
}

fn visit(value: &CommandValue, depth: usize, nodes: &mut usize) -> bool {
    if depth > MAX_DEPTH || *nodes == MAX_ELEMENTS {
        return false;
    }
    *nodes += 1;
    match value {
        CommandValue::Record(fields) => fields.values().all(|v| visit(v, depth + 1, nodes)),
        CommandValue::Page { rows, .. } => rows.iter().all(|v| visit(v, depth + 1, nodes)),
        _ => true,
    }
}

pub(super) fn fits(result: &OperationResult) -> bool {
    let mut nodes = 0;
    if !result
        .receipt
        .effects
        .iter()
        .all(|v| visit(v, 0, &mut nodes))
    {
        return false;
    }
    let shape = match &result.outcome {
        Ok(Payload::Property(p)) => [
            p.requested.as_ref(),
            p.effective.as_ref(),
            p.default.as_ref(),
            p.startup.as_ref().map(|s| &s.value),
            p.session_override.as_ref(),
        ]
        .into_iter()
        .flatten()
        .all(|v| visit(v, 0, &mut nodes)),
        Ok(Payload::Value(v)) => visit(v, 0, &mut nodes),
        _ => true,
    };
    shape
        && ron::ser::to_writer(
            Budget(MAX_PAYLOAD_BYTES),
            &(&result.outcome, &result.receipt.effects),
        )
        .is_ok()
        && ron::ser::to_writer(Budget(MAX_ADMIN_BYTES), result).is_ok()
}
