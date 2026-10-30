use super::editor::Editor;
use super::suggestions::Suggestions;
use super::text::{self, DrawContext, QuadDraw, TextDraw, UiVertex};
use super::theme::{GLYPH_ADVANCE, INPUT_TEXT};
use std::time::Instant;

const X: f32 = 46.0;

pub(super) fn draw(
    vertices: &mut Vec<UiVertex>,
    context: DrawContext,
    editor: &mut Editor,
    suggestions: &Suggestions,
) {
    let columns = ((context.width - X - 28.0) / GLYPH_ADVANCE).max(1.0) as usize;
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
    let chars: Vec<char> = editor.text.chars().collect();
    let range = editor.selection();
    let mut byte = 0;
    for (index, ch) in chars.iter().enumerate() {
        if index >= editor.scroll
            && index < editor.scroll + columns
            && byte >= range.start
            && byte < range.end
        {
            text::append_quad(
                vertices,
                context,
                QuadDraw::solid(
                    X + (index - editor.scroll) as f32 * GLYPH_ADVANCE,
                    y - 2.0,
                    GLYPH_ADVANCE,
                    24.0,
                    [0.22, 0.37, 0.52],
                ),
            );
        }
        byte += ch.len_utf8();
    }
    let visible: String = chars.iter().skip(editor.scroll).take(columns).collect();
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
    let cursor = editor.text[..editor.cursor].chars().count();
    if Instant::now().duration_since(editor.changed_at).as_millis() % 1000 < 550 {
        text::append_quad(
            vertices,
            context,
            QuadDraw::solid(
                X + (cursor - editor.scroll) as f32 * GLYPH_ADVANCE,
                y - 1.0,
                2.0,
                22.0,
                [0.96, 0.98, 1.0],
            ),
        );
    }
    for (row, hint) in suggestions.lines().enumerate() {
        text::append_text(
            vertices,
            context,
            TextDraw {
                x: 22.0,
                y: context.height - 66.0 - (row as f32 * 20.0),
                text: hint,
                color: [0.62, 0.78, 0.88],
                max_chars: ((context.width - 44.0) / GLYPH_ADVANCE) as usize,
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
    let columns = ((width as f32 - X - 28.0) / GLYPH_ADVANCE).max(1.0) as usize;
    editor.ensure_visible(columns);
    let column = ((x as f32 - X) / GLYPH_ADVANCE).round().max(0.0) as usize + editor.scroll;
    let byte = editor
        .text
        .char_indices()
        .nth(column)
        .map_or(editor.text.len(), |(at, _)| at);
    editor.move_to(byte, select);
    true
}
