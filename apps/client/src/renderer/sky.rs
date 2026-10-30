use super::environment::{EnvironmentFrame, EnvironmentFrameStatus, EnvironmentViewKind};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct EnvironmentDrawCalls {
    pub(super) sky: u32,
    pub(super) stars: u32,
    pub(super) sun: u32,
    pub(super) overview_marker: u32,
}

pub(super) fn should_draw_surface_sky(frame: &EnvironmentFrame) -> bool {
    frame.status == EnvironmentFrameStatus::Available
        && frame.render_view.kind == EnvironmentViewKind::Surface
        && frame.sky.is_some()
}

pub(super) fn should_draw_sun(frame: &EnvironmentFrame) -> bool {
    should_draw_surface_sky(frame)
        && frame
            .local_sun
            .is_some_and(|sun| sun.visible_fraction > 0.0)
}

pub(super) fn should_draw_stars(frame: &EnvironmentFrame) -> bool {
    should_draw_surface_sky(frame)
        && frame.diagnostics.star_frame.is_some()
        && frame.environment_uniform.sky_horizon_rgb_star_visibility[3] > 0.0
}

pub(super) fn clear_color(scene_visible: bool, overview_active: bool) -> wgpu::Color {
    if !scene_visible {
        return wgpu::Color::BLACK;
    }
    if overview_active {
        wgpu::Color {
            r: 0.003,
            g: 0.004,
            b: 0.009,
            a: 1.0,
        }
    } else {
        wgpu::Color {
            r: 0.008,
            g: 0.010,
            b: 0.014,
            a: 1.0,
        }
    }
}
