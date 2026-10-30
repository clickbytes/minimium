use super::theme::{ATLAS_HEIGHT, ATLAS_WIDTH};

// Rasterized Roboto Mono atlas; regenerate with scripts/generate_console_atlas.py.
pub fn font_atlas_pixels() -> Vec<u8> {
    let pixels = include_bytes!("../../assets/fonts/console-atlas.bin");
    assert_eq!(pixels.len(), ATLAS_WIDTH * ATLAS_HEIGHT);
    pixels.to_vec()
}
