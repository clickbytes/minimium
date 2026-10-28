use std::time::Duration;

pub(super) const CELL: usize = 8;
pub(super) const COLUMNS: usize = 16;
pub(super) const ASCII_GLYPHS: usize = 127;
pub(super) const POLISH_GLYPHS: [char; 9] = ['Ą', 'Ć', 'Ę', 'Ł', 'Ń', 'Ó', 'Ś', 'Ź', 'Ż'];
pub(super) const WHITE_CELL: usize = ASCII_GLYPHS + POLISH_GLYPHS.len();
pub(super) const ATLAS_CELLS: usize = WHITE_CELL + 1;
pub(super) const ATLAS_ROWS: usize = ATLAS_CELLS.div_ceil(COLUMNS);
pub(crate) const ATLAS_WIDTH: usize = CELL * COLUMNS;
pub(crate) const ATLAS_HEIGHT: usize = CELL * ATLAS_ROWS;
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
        (WHITE_CELL % COLUMNS * CELL + 3) as f32 / ATLAS_WIDTH as f32,
        (WHITE_CELL / COLUMNS * CELL + 3) as f32 / ATLAS_HEIGHT as f32,
    ]
}
