use super::editor::Editor;
use super::suggestions::Suggestions;
use super::text::{self, DrawContext, QuadDraw, TextDraw, UiVertex};
use super::theme::INPUT_TEXT;
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

const X: f32 = 46.0;

pub(super) fn draw(
    vertices: &mut Vec<UiVertex>,
    context: DrawContext,
    editor: &mut Editor,
    suggestions: &Suggestions,
) {
    let advance = text::glyph_advance();
    let columns = ((context.width - X - 28.0) / advance).max(1.0) as usize;
    editor.ensure_visible(columns);
    let y = context.height - 42.0;
    text::append_text(
        vertices,
        context,
        TextDraw {
            x: 22.0,
            y,
            text: "> ",
            color: INPUT_TEXT,
            max_chars: 2,
            scale: 1.0,
        },
    );
    let prefix = &editor.text[..editor.cursor];
    let suffix = &editor.text[editor.cursor..];
    let display_text = format!("{prefix}{}{suffix}", editor.preedit);
    let chars: Vec<&str> = display_text.graphemes(true).collect();
    let range = editor.selection();
    let mut byte = 0;
    for (index, grapheme) in chars.iter().enumerate() {
        if editor.preedit.is_empty()
            && index >= editor.scroll
            && index < editor.scroll + columns
            && byte >= range.start
            && byte < range.end
        {
            text::append_quad(
                vertices,
                context,
                QuadDraw::solid(
                    X + (index - editor.scroll) as f32 * advance,
                    y - 2.0,
                    advance,
                    24.0,
                    [0.22, 0.37, 0.52],
                ),
            );
        }
        byte += grapheme.len();
    }
    let visible = chars
        .iter()
        .skip(editor.scroll)
        .take(columns)
        .copied()
        .collect::<Vec<_>>()
        .concat();
    text::append_text(
        vertices,
        context,
        TextDraw {
            x: X,
            y,
            text: &visible,
            color: INPUT_TEXT,
            max_chars: columns,
            scale: 1.0,
        },
    );
    let cursor = prefix.graphemes(true).count() + editor.preedit.graphemes(true).count();
    if !editor.preedit.is_empty() {
        let start = prefix.graphemes(true).count().saturating_sub(editor.scroll);
        let width = editor.preedit.graphemes(true).count() as f32 * advance;
        text::append_quad(
            vertices,
            context,
            QuadDraw::solid(X + start as f32 * advance, y + 20.0, width, 1.5, INPUT_TEXT),
        );
    }
    if Instant::now().duration_since(editor.changed_at).as_millis() % 1000 < 550 {
        text::append_quad(
            vertices,
            context,
            QuadDraw::solid(
                X + (cursor - editor.scroll) as f32 * advance,
                y - 1.0,
                2.0,
                22.0,
                [0.96, 0.98, 1.0],
            ),
        );
    }
    for (row, hint) in suggestions.lines().into_iter().enumerate() {
        text::append_text(
            vertices,
            context,
            TextDraw {
                x: 22.0,
                y: context.height - 66.0 - (row as f32 * 20.0),
                text: &hint,
                color: if row == 0 {
                    [0.80, 0.88, 0.94]
                } else {
                    [0.61, 0.70, 0.77]
                },
                max_chars: ((context.width - 44.0) / advance) as usize,
                scale: 1.0,
            },
        );
    }
}

pub(super) fn hit(
    editor: &mut Editor,
    width: u32,
    height: u32,
    x: f64,
    y: f64,
    select: bool,
) -> bool {
    if !select
        && (y < height as f64 - 48.0
            || y > height as f64 - 18.0
            || x < 16.0
            || x > width as f64 - 16.0)
    {
        return false;
    }
    let advance = text::glyph_advance();
    let columns = ((width as f32 - X - 28.0) / advance).max(1.0) as usize;
    editor.ensure_visible(columns);
    let column = ((x as f32 - X) / advance).round().max(0.0) as usize + editor.scroll;
    let byte = editor
        .text
        .grapheme_indices(true)
        .nth(column)
        .map_or(editor.text.len(), |(at, _)| at);
    editor.move_to(byte, select);
    true
}
