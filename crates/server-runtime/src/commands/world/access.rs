//! Shared page admission for legacy and canonical world reads.

use super::{invalid, CommandReply, CommandViewRequest, ModuleCommandContext};

pub(super) fn page(
    context: &ModuleCommandContext<'_>,
    args: &[&str],
    legacy_max: usize,
    usage: &'static str,
) -> Result<CommandViewRequest, CommandReply> {
    let (cursor, limit, bytes) = match args {
        [] => (None, 256, minimium_gameplay::MAX_COMMAND_VIEW_BYTES),
        [cursor] if legacy_max == 1 => (
            Some(*cursor),
            256,
            minimium_gameplay::MAX_COMMAND_VIEW_BYTES,
        ),
        [cursor, limit] => {
            let limit = limit.parse::<usize>().map_err(|_| invalid(usage))?;
            (Some(*cursor), limit, 32 * 1024)
        }
        _ => return Err(invalid(usage)),
    };
    let mut request = CommandViewRequest::new(limit, bytes).map_err(|_| invalid(usage))?;
    if let Some(token) = cursor.filter(|token| *token != "none") {
        request = request.after(context.cursor(token).map_err(|_| {
            CommandReply::error("server.command.stale_cursor", "cursor is invalid or stale")
        })?);
    }
    Ok(request)
}
