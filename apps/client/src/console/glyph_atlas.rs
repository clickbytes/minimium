use super::theme::{
    ATLAS_HEIGHT, ATLAS_WIDTH, CELL, COLUMNS, GLYPH_COUNT, MAX_ATLAS_BYTES, WHITE_CELL,
};
use fontdue::{Font, FontSettings};
use std::{collections::HashMap, sync::OnceLock};

#[derive(Clone, Copy)]
pub(super) struct Glyph {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) xmin: i32,
    pub(super) ymin: i32,
    pub(super) advance: f32,
    pub(super) uv_min: [f32; 2],
    pub(super) uv_max: [f32; 2],
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct GlyphKey {
    font_hash: usize,
    size_bits: u32,
    character: char,
}

struct GlyphAtlas {
    pixels: Vec<u8>,
    glyphs: HashMap<GlyphKey, Glyph>,
    font_hash: usize,
    size_bits: u32,
    baseline: f32,
}

static GLYPH_ATLAS: OnceLock<GlyphAtlas> = OnceLock::new();
const FONT_SIZE_PX: f32 = 20.0;

pub(super) fn pixels() -> Vec<u8> {
    atlas().pixels.clone()
}

pub(super) fn get(character: char) -> Glyph {
    let atlas = atlas();
    atlas
        .glyphs
        .get(&key(atlas, character))
        .or_else(|| atlas.glyphs.get(&key(atlas, '?')))
        .copied()
        .expect("replacement glyph must be present")
}

pub(super) fn advance() -> f32 {
    get('M').advance
}

pub(super) fn baseline() -> f32 {
    atlas().baseline
}

fn key(atlas: &GlyphAtlas, character: char) -> GlyphKey {
    GlyphKey {
        font_hash: atlas.font_hash,
        size_bits: atlas.size_bits,
        character,
    }
}

fn atlas() -> &'static GlyphAtlas {
    GLYPH_ATLAS.get_or_init(build)
}

fn build() -> GlyphAtlas {
    let font = Font::from_bytes(
        include_bytes!("../../assets/fonts/RobotoMono.ttf").as_slice(),
        FontSettings::default(),
    )
    .expect("bundled console font must be valid");
    let font_hash = font.file_hash();
    let size_bits = FONT_SIZE_PX.to_bits();
    let baseline = font
        .horizontal_line_metrics(FONT_SIZE_PX)
        .expect("bundled font must expose horizontal line metrics")
        .ascent;
    assert!(
        ATLAS_WIDTH * ATLAS_HEIGHT <= MAX_ATLAS_BYTES,
        "fixed console glyph atlas budget exceeded"
    );
    let mut pixels = vec![0; ATLAS_WIDTH * ATLAS_HEIGHT];
    let requested_characters = (0x20..=0x24f)
        .chain(0x300..=0x36f)
        .chain(0x2000..=0x206f)
        .chain(std::iter::once(0xfffd))
        .filter_map(char::from_u32)
        .collect::<Vec<_>>();
    let mut glyphs = HashMap::with_capacity(GLYPH_COUNT);
    for requested in requested_characters {
        let rasterized = if font.has_glyph(requested) {
            requested
        } else {
            '?'
        };
        let (metrics, bitmap) = font.rasterize(rasterized, FONT_SIZE_PX);
        let index = glyphs.len();
        let cell_x = index % COLUMNS * CELL;
        let cell_y = index / COLUMNS * CELL;
        assert!(
            metrics.width <= CELL - 2 && metrics.height <= CELL - 2,
            "font glyph {requested:?} exceeds its padded atlas cell"
        );
        let width = metrics.width;
        let height = metrics.height;
        for row in 0..height {
            let source = row * metrics.width;
            let target = (cell_y + 1 + row) * ATLAS_WIDTH + cell_x + 1;
            pixels[target..target + width].copy_from_slice(&bitmap[source..source + width]);
        }
        glyphs.insert(
            GlyphKey {
                font_hash,
                size_bits,
                character: requested,
            },
            Glyph {
                width,
                height,
                xmin: metrics.xmin,
                ymin: metrics.ymin,
                advance: metrics.advance_width,
                uv_min: [
                    (cell_x + 1) as f32 / ATLAS_WIDTH as f32,
                    (cell_y + 1) as f32 / ATLAS_HEIGHT as f32,
                ],
                uv_max: [
                    (cell_x + 1 + width) as f32 / ATLAS_WIDTH as f32,
                    (cell_y + 1 + height) as f32 / ATLAS_HEIGHT as f32,
                ],
            },
        );
    }
    let white_x = WHITE_CELL % COLUMNS * CELL + 3;
    let white_y = WHITE_CELL / COLUMNS * CELL + 3;
    pixels[white_y * ATLAS_WIDTH + white_x] = 255;
    GlyphAtlas {
        pixels,
        glyphs,
        font_hash,
        size_bits,
        baseline,
    }
}
