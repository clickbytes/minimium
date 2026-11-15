use super::text::{self, DrawContext, QuadDraw, TextDraw, UiVertex};
use super::theme::{white_uv, MAX_VERTICES};

pub struct ToolOverlay {
    pub heading: String,
    pub target: String,
    pub status: String,
    pub controls: String,
    pub preview: String,
    pub result: Option<String>,
    pub color: [f32; 3],
    /// Ring vertices in normalized device coordinates; missing points are clipped.
    pub outline: Vec<Option<[f32; 2]>>,
}

pub(super) fn append(vertices: &mut Vec<UiVertex>, context: DrawContext, overlay: &ToolOverlay) {
    for pair in overlay.outline.windows(2) {
        if let [Some(a), Some(b)] = pair {
            append_line(vertices, context, *a, *b, overlay.color);
        }
    }
    for (x, y, w, h) in [
        (-10.0, -1.0, 7.0, 2.0),
        (3.0, -1.0, 7.0, 2.0),
        (-1.0, -10.0, 2.0, 7.0),
        (-1.0, 3.0, 2.0, 7.0),
    ] {
        text::append_quad(
            vertices,
            context,
            QuadDraw::solid(
                context.width * 0.5 + x,
                context.height * 0.5 + y,
                w,
                h,
                overlay.color,
            ),
        );
    }
    let width = (context.width - 32.0).min(1040.0);
    let scale = 0.75;
    let columns = ((width - 20.0) / (text::glyph_advance() * scale)).max(1.0) as usize;
    let mut lines = Vec::new();
    for (text, color) in [
        (overlay.heading.as_str(), [0.94, 0.95, 0.97]),
        (overlay.target.as_str(), [0.84, 0.86, 0.90]),
        (overlay.status.as_str(), overlay.color),
        (overlay.preview.as_str(), [0.65, 0.70, 0.77]),
        (overlay.controls.as_str(), [0.84, 0.86, 0.90]),
        (overlay.result.as_deref().unwrap_or(""), [1.0, 0.78, 0.42]),
    ] {
        let characters: Vec<_> = text.chars().collect();
        for row in characters.chunks(columns) {
            lines.push((row.iter().collect::<String>(), color));
        }
    }
    let maximum_rows = ((context.height - 130.0) / 20.0).max(1.0) as usize;
    lines.truncate(maximum_rows);
    text::append_quad(
        vertices,
        context,
        QuadDraw::solid(
            12.0,
            56.0,
            width,
            16.0 + lines.len() as f32 * 20.0,
            [0.025, 0.035, 0.050],
        ),
    );
    for (index, (line, color)) in lines.iter().enumerate() {
        text::append_text(
            vertices,
            context,
            TextDraw {
                x: 22.0,
                y: 64.0 + index as f32 * 20.0,
                text: line,
                color: *color,
                max_chars: columns,
                scale,
            },
        );
    }
}

fn append_line(
    vertices: &mut Vec<UiVertex>,
    context: DrawContext,
    a: [f32; 2],
    b: [f32; 2],
    color: [f32; 3],
) {
    if vertices.len() + 6 > MAX_VERTICES {
        return;
    }
    let delta = [
        (b[0] - a[0]) * context.width,
        (b[1] - a[1]) * context.height,
    ];
    let length = delta[0].hypot(delta[1]);
    if length < 0.01 {
        return;
    }
    let offset = [
        -delta[1] / length * 2.0 / context.width,
        delta[0] / length * 2.0 / context.height,
    ];
    let point = |p: [f32; 2], sign: f32| UiVertex {
        position: [p[0] + sign * offset[0], p[1] + sign * offset[1]],
        uv: white_uv(),
        color,
    };
    vertices.extend([
        point(a, 1.0),
        point(a, -1.0),
        point(b, -1.0),
        point(a, 1.0),
        point(b, -1.0),
        point(b, 1.0),
    ]);
}
