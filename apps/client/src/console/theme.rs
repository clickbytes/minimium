use std::time::Duration;

pub(super) const CELL: usize = 32;
pub(super) const COLUMNS: usize = 16;
pub(super) const GLYPH_COUNT: usize =
    (0x24f - 0x20 + 1) + (0x36f - 0x300 + 1) + (0x206f - 0x2000 + 1) + 1;
pub(super) const WHITE_CELL: usize = GLYPH_COUNT;
pub(super) const ATLAS_CELLS: usize = WHITE_CELL + 1;
pub(super) const ATLAS_ROWS: usize = ATLAS_CELLS.div_ceil(COLUMNS);
pub(crate) const ATLAS_WIDTH: usize = CELL * COLUMNS;
pub(crate) const ATLAS_HEIGHT: usize = CELL * ATLAS_ROWS;
pub(super) const MAX_ATLAS_BYTES: usize = 1024 * 1024;
pub(super) const MAX_LINES: usize = 128;
pub(super) const MAX_CHAT_LINES: usize = 8;
pub(super) const CHAT_VISIBLE_DURATION: Duration = Duration::from_secs(6);
pub(super) const MAX_INPUT: usize = 128;
pub(super) const MAX_VERTICES: usize = 8192;

pub(super) const CONSOLE_TEXT: [f32; 3] = [0.78, 0.86, 0.92];
pub(super) const ERROR_TEXT: [f32; 3] = [1.0, 0.40, 0.34];
pub(super) const CHAT_TEXT: [f32; 3] = [0.76, 0.88, 0.96];
pub(super) const ECHO_TEXT: [f32; 3] = [0.35, 0.88, 0.88];
pub(super) const INPUT_TEXT: [f32; 3] = [0.88, 0.96, 0.98];

pub(super) fn white_uv() -> [f32; 2] {
    [
        (WHITE_CELL % COLUMNS * CELL) as f32 / ATLAS_WIDTH as f32 + 3.5 / ATLAS_WIDTH as f32,
        (WHITE_CELL / COLUMNS * CELL) as f32 / ATLAS_HEIGHT as f32 + 3.5 / ATLAS_HEIGHT as f32,
    ]
}
