use super::{
    editor::Editor,
    history,
    state::Console,
    suggestions::Suggestions,
    text::{self, DrawContext, QuadDraw, TextDraw, UiVertex},
    theme::*,
};
use minimium_commands::completion::ItemKind;
use minimium_localization::{Locale, Localizer};
use unicode_segmentation::UnicodeSegmentation;

impl Console {
    pub(crate) fn submission(&mut self, localizer: &Localizer, locale: Locale) -> Option<String> {
        if !self.suggestions.ready(&self.editor, localizer, locale) {
            return None;
        }
        Some(history::take_input(self, localizer, locale))
    }

    pub(crate) fn dismiss_popup(&mut self) -> bool {
        self.suggestions.dismiss()
    }
    pub(crate) fn select_popup(&mut self, back: bool) -> bool {
        self.suggestions.select(back)
    }
    pub(crate) fn complete_input(&mut self, back: bool, localizer: &Localizer, locale: Locale) {
        self.suggestions
            .complete(&mut self.editor, back, localizer, locale);
    }
}

pub(super) fn draw(
    vertices: &mut Vec<UiVertex>,
    ctx: DrawContext,
    editor: &Editor,
    popup: &Suggestions,
) {
    let Some(result) = popup.result.as_ref() else {
        return;
    };
    if result.buffer_revision != editor.revision || popup.row_count() == 0 {
        return;
    }
    let advance = text::glyph_advance();
    let anchor = popup
        .diagnostic
        .as_ref()
        .map(|(_, s)| s.start)
        .or_else(|| {
            result
                .items
                .get(popup.selected)
                .map(|i| i.replace_range.start)
        })
        .unwrap_or(editor.cursor);
    let column = |at| {
        editor
            .text
            .get(..at)
            .map_or(0, |s| s.graphemes(true).count())
            .saturating_sub(editor.scroll)
    };
    let token_x = 46.0 + column(anchor) as f32 * advance;
    let width = (ctx.width - 16.0).min(80.0 * advance + 12.0);
    let x = token_x.clamp(8.0, (ctx.width - width - 8.0).max(8.0));
    let row_height = 22.0;
    let bottom = ctx.height - 48.0;
    let available = ((bottom - 4.0) / row_height).floor().max(1.0) as usize;
    let rows = available.min(popup.row_count());
    let y = bottom - rows as f32 * row_height;
    let columns = ((width - 12.0) / advance) as usize;
    let mut row = |i: usize, label: &str, color: [f32; 3], selected: bool| {
        let top = y + i as f32 * row_height;
        text::append_quad(
            vertices,
            ctx,
            QuadDraw::solid(
                x,
                top,
                width,
                row_height,
                if selected { POPUP_SELECTED } else { POPUP_BG },
            ),
        );
        if selected {
            text::append_quad(
                vertices,
                ctx,
                QuadDraw::solid(x, top, 3.0, row_height, POPUP_SCOPE),
            );
        }
        text::append_text(
            vertices,
            ctx,
            TextDraw {
                x: x + 7.0,
                y: top,
                text: label,
                color,
                max_chars: columns,
                scale: 1.0,
            },
        );
    };
    if let Some((error, span)) = &popup.diagnostic {
        row(0, error, ERROR_TEXT, false);
        let start = token_x.min(ctx.width - 12.0);
        let end = (46.0 + column(span.end) as f32 * advance).min(ctx.width - 8.0);
        text::append_quad(
            vertices,
            ctx,
            QuadDraw::solid(
                start,
                ctx.height - 20.0,
                (end - start).max(3.0),
                2.0,
                ERROR_TEXT,
            ),
        );
        return;
    }
    let signature_rows =
        usize::from(popup.signature.is_some() && (rows > 1 || popup.labels.is_empty()));
    let visible = rows - signature_rows;
    let first = (popup.selected + 1).saturating_sub(visible);
    for (i, item) in result.items.iter().enumerate().skip(first).take(visible) {
        let color = match item.kind {
            ItemKind::Property => POPUP_FIELD,
            ItemKind::Method => POPUP_METHOD,
            _ => POPUP_SCOPE,
        };
        row(i - first, &popup.labels[i], color, i == popup.selected);
    }
    if signature_rows == 1 {
        row(
            visible,
            popup.signature.as_deref().unwrap_or(""),
            POPUP_SCOPE,
            false,
        );
    }
}
