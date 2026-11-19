use super::theme::MAX_VERTICES;

// Nine compact rows leave enough room for every byte of input, IME and focus quads.
pub(super) const POPUP_COLUMNS: usize = 76;

pub(super) fn background(input_vertices: usize) -> usize {
    MAX_VERTICES.saturating_sub(input_vertices) / 6 * 6
}
