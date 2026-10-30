use super::text::{self, DrawContext, QuadDraw, TextDraw, UiVertex};

// overview_zoom is the camera distance in planet radii: smaller means closer.
pub(crate) const MIN_ZOOM: f32 = 1.02;
pub(crate) const MAX_ZOOM: f32 = 8.0;

fn geometry(width: f32, height: f32) -> (f32, f32, f32) {
    let length = (height * 0.45).clamp(100.0, 320.0);
    (width - 38.0, (height - length) * 0.5, length)
}

fn fraction(zoom: f32) -> f32 {
    (MAX_ZOOM / zoom.clamp(MIN_ZOOM, MAX_ZOOM)).ln() / (MAX_ZOOM / MIN_ZOOM).ln()
}

pub(crate) fn slider_value_at_y(height: u32, y: f64) -> f32 {
    let (_, top, length) = geometry(0.0, height as f32);
    let fraction = ((top + length - y as f32) / length).clamp(0.0, 1.0);
    MAX_ZOOM * (MIN_ZOOM / MAX_ZOOM).powf(fraction)
}

pub(crate) fn slider_hit(width: u32, height: u32, x: f64, y: f64) -> bool {
    if width < 120 || height < 180 {
        return false;
    }
    let (center, top, length) = geometry(width as f32, height as f32);
    (x as f32 - center).abs() <= 22.0 && (top - 18.0..=top + length + 18.0).contains(&(y as f32))
}

pub(super) fn draw(vertices: &mut Vec<UiVertex>, context: DrawContext, zoom: f32) {
    let (center, top, length) = geometry(context.width, context.height);
    text::append_quad(
        vertices,
        context,
        QuadDraw::solid(
            center - 19.0,
            top - 38.0,
            38.0,
            length + 76.0,
            [0.025, 0.035, 0.05],
        ),
    );
    text::append_quad(
        vertices,
        context,
        QuadDraw::solid(center - 2.0, top, 4.0, length, [0.35, 0.43, 0.49]),
    );
    let thumb_y = top + length * (1.0 - fraction(zoom));
    text::append_quad(
        vertices,
        context,
        QuadDraw::solid(center - 12.0, thumb_y - 5.0, 24.0, 10.0, [0.92, 0.95, 0.98]),
    );
    let magnification = (1.08 - 1.0) / (zoom - 1.0);
    let label = format!("{magnification:.2}x");
    text::append_text(
        vertices,
        context,
        TextDraw {
            x: center - 18.0,
            y: top + length + 16.0,
            text: &label,
            color: [0.92, 0.95, 0.98],
            max_chars: 6,
            scale: 0.75,
        },
    );
}
