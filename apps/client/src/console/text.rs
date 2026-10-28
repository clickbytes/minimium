use super::theme::{
    ASCII_GLYPHS, ATLAS_HEIGHT, ATLAS_WIDTH, CELL, COLUMNS, MAX_VERTICES, POLISH_GLYPHS,
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 3],
}

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
        let uv = super::theme::white_uv();
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

pub(super) fn append_text(vertices: &mut Vec<UiVertex>, context: DrawContext, draw: TextDraw<'_>) {
    for (column, character) in draw.text.chars().take(draw.max_chars).enumerate() {
        if vertices.len() + 6 > MAX_VERTICES {
            return;
        }
        let code = glyph_index(character);
        let cell_x = (code % COLUMNS) * CELL;
        let cell_y = (code / COLUMNS) * CELL;
        let uv_min = [
            (cell_x as f32 + 0.5) / ATLAS_WIDTH as f32,
            (cell_y as f32 + 0.5) / ATLAS_HEIGHT as f32,
        ];
        let uv_max = [
            (cell_x as f32 + CELL as f32 - 0.5) / ATLAS_WIDTH as f32,
            (cell_y as f32 + CELL as f32 - 0.5) / ATLAS_HEIGHT as f32,
        ];
        let left = draw.x + column as f32 * CELL as f32 * draw.scale;
        append_quad(
            vertices,
            context,
            QuadDraw {
                x: left,
                y: draw.y,
                width: CELL as f32 * draw.scale,
                height: CELL as f32 * draw.scale,
                color: draw.color,
                uv_min,
                uv_max,
            },
        );
    }
}

fn glyph_index(character: char) -> usize {
    let normalized = match character {
        'ą' => 'Ą',
        'ć' => 'Ć',
        'ę' => 'Ę',
        'ł' => 'Ł',
        'ń' => 'Ń',
        'ó' => 'Ó',
        'ś' => 'Ś',
        'ź' => 'Ź',
        'ż' => 'Ż',
        _ => character.to_ascii_uppercase(),
    };
    if normalized.is_ascii() && (normalized as usize) < ASCII_GLYPHS {
        normalized as usize
    } else {
        POLISH_GLYPHS
            .iter()
            .position(|&glyph| glyph == normalized)
            .map_or('?' as usize, |index| ASCII_GLYPHS + index)
    }
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
