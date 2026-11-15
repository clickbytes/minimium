use super::glyph_atlas::{self, Glyph};
use super::theme::{ATLAS_HEIGHT, ATLAS_WIDTH, CELL, COLUMNS, MAX_VERTICES, WHITE_CELL};
pub(super) use minimium_renderer::UiVertex;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy)]
pub(super) struct DrawContext {
    pub(super) width: f32,
    pub(super) height: f32,
}

impl DrawContext {
    pub(super) fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Copy)]
pub(super) struct TextDraw<'a> {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) text: &'a str,
    pub(super) color: [f32; 3],
    pub(super) max_chars: usize,
    pub(super) scale: f32,
}

pub(super) struct QuadDraw {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) color: [f32; 3],
    pub(super) uv_min: [f32; 2],
    pub(super) uv_max: [f32; 2],
}

impl QuadDraw {
    pub(super) fn solid(x: f32, y: f32, width: f32, height: f32, color: [f32; 3]) -> Self {
        let uv = white_uv();
        Self {
            x,
            y,
            width,
            height,
            color,
            uv_min: uv,
            uv_max: uv,
        }
    }
}

pub(super) fn glyph_advance() -> f32 {
    glyph_atlas::advance()
}

pub(super) fn append_text(vertices: &mut Vec<UiVertex>, context: DrawContext, draw: TextDraw<'_>) {
    let mut pen_x = draw.x;
    let baseline = draw.y + glyph_atlas::baseline() * draw.scale;
    let mut last_base_x = pen_x;
    for grapheme in draw.text.graphemes(true).take(draw.max_chars) {
        for character in grapheme.chars() {
            if vertices.len() + 6 > MAX_VERTICES {
                return;
            }
            let glyph = glyph_atlas::get(character);
            append_glyph(vertices, context, glyph, draw, pen_x, last_base_x, baseline);
            if glyph.advance != 0.0 {
                last_base_x = pen_x;
                pen_x += glyph.advance * draw.scale;
            }
        }
    }
}

fn append_glyph(
    vertices: &mut Vec<UiVertex>,
    context: DrawContext,
    glyph: Glyph,
    draw: TextDraw<'_>,
    pen_x: f32,
    last_base_x: f32,
    baseline: f32,
) {
    if glyph.width == 0 || glyph.height == 0 {
        return;
    }
    let glyph_x = if glyph.advance == 0.0 {
        last_base_x
    } else {
        pen_x
    };
    append_quad(
        vertices,
        context,
        QuadDraw {
            x: glyph_x + glyph.xmin as f32 * draw.scale,
            y: baseline - (glyph.ymin + glyph.height as i32) as f32 * draw.scale,
            width: glyph.width as f32 * draw.scale,
            height: glyph.height as f32 * draw.scale,
            color: draw.color,
            uv_min: glyph.uv_min,
            uv_max: glyph.uv_max,
        },
    );
}

fn white_uv() -> [f32; 2] {
    [
        (WHITE_CELL % COLUMNS * CELL) as f32 / ATLAS_WIDTH as f32 + 3.5 / ATLAS_WIDTH as f32,
        (WHITE_CELL / COLUMNS * CELL) as f32 / ATLAS_HEIGHT as f32 + 3.5 / ATLAS_HEIGHT as f32,
    ]
}

pub(super) fn append_quad(vertices: &mut Vec<UiVertex>, context: DrawContext, draw: QuadDraw) {
    if vertices.len() + 6 > MAX_VERTICES {
        return;
    }
    let top_left = screen_position(draw.x, draw.y, context);
    let top_right = screen_position(draw.x + draw.width, draw.y, context);
    let bottom_left = screen_position(draw.x, draw.y + draw.height, context);
    let bottom_right = screen_position(draw.x + draw.width, draw.y + draw.height, context);
    let make = |position, uv| UiVertex {
        position,
        uv,
        color: draw.color,
    };
    vertices.extend_from_slice(&[
        make(top_left, [draw.uv_min[0], draw.uv_min[1]]),
        make(bottom_left, [draw.uv_min[0], draw.uv_max[1]]),
        make(bottom_right, [draw.uv_max[0], draw.uv_max[1]]),
        make(top_left, [draw.uv_min[0], draw.uv_min[1]]),
        make(bottom_right, [draw.uv_max[0], draw.uv_max[1]]),
        make(top_right, [draw.uv_max[0], draw.uv_min[1]]),
    ]);
}

fn screen_position(x: f32, y: f32, context: DrawContext) -> [f32; 2] {
    [
        x / context.width * 2.0 - 1.0,
        1.0 - y / context.height * 2.0,
    ]
}
