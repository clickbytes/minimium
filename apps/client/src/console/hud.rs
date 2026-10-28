use super::text::{self, DrawContext, QuadDraw, TextDraw, UiVertex};
use super::theme::white_uv;

pub(super) struct HudState {
    pub(super) health: u16,
    pub(super) show_crosshair: bool,
    pub(super) hit_feedback: bool,
}

pub(super) fn append_hud(vertices: &mut Vec<UiVertex>, context: DrawContext, state: HudState) {
    let health_ratio = state.health.min(100) as f32 / 100.0;
    let health_color = if health_ratio > 0.5 {
        [0.20, 0.85, 0.38]
    } else if health_ratio > 0.25 {
        [0.96, 0.72, 0.20]
    } else {
        [0.95, 0.24, 0.20]
    };
    let mut health_label = *b"HP 000/100";
    let health_digits = state.health.min(100);
    if health_digits == 100 {
        health_label[3..6].copy_from_slice(b"100");
    } else if health_digits >= 10 {
        health_label[4] = b'0' + (health_digits / 10) as u8;
        health_label[5] = b'0' + (health_digits % 10) as u8;
    } else {
        health_label[4] = b' ';
        health_label[5] = b'0' + health_digits as u8;
    }
    text::append_text(
        vertices,
        context,
        TextDraw {
            x: 16.0,
            y: context.height - 58.0,
            text: std::str::from_utf8(&health_label).unwrap_or("HP"),
            color: [0.90, 0.95, 0.98],
            max_chars: 12,
            scale: 2.0,
        },
    );
    text::append_quad(
        vertices,
        context,
        QuadDraw::solid(
            16.0,
            context.height - 34.0,
            164.0,
            10.0,
            [0.025, 0.035, 0.050],
        ),
    );
    if health_ratio > 0.0 {
        text::append_quad(
            vertices,
            context,
            QuadDraw::solid(
                18.0,
                context.height - 32.0,
                160.0 * health_ratio,
                6.0,
                health_color,
            ),
        );
    }

    if state.show_crosshair {
        let center_x = context.width * 0.5;
        let center_y = context.height * 0.5;
        let color = if state.hit_feedback {
            [1.0, 0.36, 0.18]
        } else {
            [0.90, 0.95, 0.98]
        };
        let uv = white_uv();
        for (x, y, width, height) in [
            (center_x - 8.0, center_y - 1.0, 6.0, 2.0),
            (center_x + 2.0, center_y - 1.0, 6.0, 2.0),
            (center_x - 1.0, center_y - 8.0, 2.0, 6.0),
            (center_x - 1.0, center_y + 2.0, 2.0, 6.0),
        ] {
            text::append_quad(
                vertices,
                context,
                QuadDraw {
                    x,
                    y,
                    width,
                    height,
                    color,
                    uv_min: uv,
                    uv_max: uv,
                },
            );
        }
    }
}
