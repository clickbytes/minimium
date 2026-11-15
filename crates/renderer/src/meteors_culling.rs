//! Conservative segment bounds, matching the vertex shader's expanded ribbons.
use crate::{meteors::MeteorFrame, RenderViewFrame};
use glam::{Mat4, Vec3};

pub(crate) fn visible(
    start: [f32; 4],
    end: [f32; 4],
    optical: bool,
    frame: &MeteorFrame,
    view: &RenderViewFrame,
) -> bool {
    let mut a = Vec3::from_slice(&start);
    let mut b = Vec3::from_slice(&end);
    let matrix = Mat4::from_cols_array_2d(&view.view_projection);
    let wa = (matrix * a.extend(1.0)).w;
    let wb = (matrix * b.extend(1.0)).w;
    if wa < 0.01 && wb < 0.01 {
        return false;
    }
    if wa < 0.01 {
        a = a.lerp(b, (0.01 - wa) / (wb - wa));
    }
    if wb < 0.01 {
        b = b.lerp(a, (0.01 - wb) / ((matrix * a.extend(1.0)).w - wb));
    }
    let ca = matrix * a.extend(1.0);
    let cb = matrix * b.extend(1.0);
    let norm = matrix
        .row(0)
        .truncate()
        .length()
        .max(matrix.row(1).truncate().length());
    // The shader adds 32 pixels at either edge. A 64-pixel margin based on
    // the smaller viewport dimension is deliberately conservative.
    let viewport = view.viewport[0].min(view.viewport[1]).max(1) as f32;
    let mut margin =
        start[3].max(end[3]) * norm / ca.w.min(cb.w).max(0.001) * 4.0 + 64.0 / viewport;
    if optical {
        margin += norm * 0.006 * 4.0;
    }
    let pa = ca.truncate().truncate() / ca.w.max(0.001);
    let pb = cb.truncate().truncate() / cb.w.max(0.001);
    if pa.min(pb).min_element().is_nan() {
        return true;
    }
    if pa.min(pb).x > 1.0 + margin
        || pa.min(pb).y > 1.0 + margin
        || pa.max(pb).x < -1.0 - margin
        || pa.max(pb).y < -1.0 - margin
    {
        return false;
    }
    let context = frame.context();
    let center = ((a + b) * 0.5).as_dvec3() + glam::DVec3::from_array(context.render_origin_m);
    let distance = center.distance(glam::DVec3::from_array(context.camera_eye_world_m));
    let extent = f64::from(a.distance(b) * 0.5 + start[3].max(end[3]) * 4.0)
        + if optical { distance * 0.024 } else { 0.0 };
    crate::meteors::visibility::head_visible(
        context.camera_eye_world_m,
        center.to_array(),
        context.game_radius_m,
        extent,
    )
}
