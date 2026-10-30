use super::environment::{stars::StarCatalogueAdapter, EnvironmentFrame, EnvironmentFrameStatus};
use crate::console::CelestialLabel;
use glam::{Mat4, Vec3, Vec4};
use minimium_astronomy::{SKY_MOON_ID, SKY_SUN_ID};
use minimium_core::universe::BodyKind;

const MAX_STAR_LABELS: usize = 32;
const MAX_STAR_CANDIDATES: usize = 160;

pub(super) fn prepare(
    frame: &EnvironmentFrame,
    catalogue: Option<&StarCatalogueAdapter>,
) -> Vec<CelestialLabel> {
    if frame.status != EnvironmentFrameStatus::Available {
        return Vec::new();
    }
    let mut labels = Vec::with_capacity(MAX_STAR_LABELS + 8);
    let Some(sky) = frame.sky else {
        return labels;
    };
    add_label(
        &mut labels,
        frame,
        sky.sun.apparent_direction_world,
        "Sun".into(),
    );
    add_label(
        &mut labels,
        frame,
        sky.moon.body.apparent_direction_world,
        "Moon".into(),
    );
    for body in &frame.observed_bodies {
        if !body.visible || body.id == SKY_SUN_ID || body.id == SKY_MOON_ID {
            continue;
        }
        let name = match body.id.get() {
            1001 => "Demo star".to_owned(),
            1002 => "Demo planet".to_owned(),
            1003 => "Demo moon".to_owned(),
            id => format!("{} {id}", body_kind_name(body.kind)),
        };
        add_label(&mut labels, frame, body.direction_world, name);
    }
    let Some(catalogue) = catalogue else {
        return labels;
    };
    if frame.render_view.kind == super::environment::EnvironmentViewKind::Surface
        && frame.environment_uniform.sky_horizon_rgb_star_visibility[3] < 0.08
    {
        return labels;
    }
    let eqj_to_world = Mat4::from_cols_array_2d(&frame.environment_uniform.eqj_to_world);
    let mut stars_added = 0;
    for (record, name) in catalogue
        .records()
        .iter()
        .zip(catalogue.labels())
        .take(MAX_STAR_CANDIDATES)
    {
        if stars_added == MAX_STAR_LABELS {
            break;
        }
        let direction = Vec3::from_array(record.position_eqj_pc.map(|v| v as f32));
        if !direction.is_finite() || direction.length_squared() < 1.0e-20 {
            continue;
        }
        let world = eqj_to_world
            .transform_vector3(direction.normalize())
            .normalize();
        if add_label(
            &mut labels,
            frame,
            world.as_dvec3().to_array(),
            (*name).to_owned(),
        ) {
            stars_added += 1;
        }
    }
    labels
}

fn body_kind_name(kind: BodyKind) -> &'static str {
    match kind {
        BodyKind::Star => "Star",
        BodyKind::Planet => "Planet",
        BodyKind::Moon => "Moon",
        BodyKind::MinorBody => "Body",
    }
}

fn add_label(
    labels: &mut Vec<CelestialLabel>,
    frame: &EnvironmentFrame,
    direction: [f64; 3],
    text: String,
) -> bool {
    let world = Vec3::from_array(direction.map(|v| v as f32));
    if !world.is_finite() || world.length_squared() < 1.0e-20 {
        return false;
    }
    let world = world.normalize();
    let horizon = frame.environment_uniform.observer_up_horizon_cos;
    if frame.render_view.kind == super::environment::EnvironmentViewKind::Surface
        && world.dot(Vec3::from_array([horizon[0], horizon[1], horizon[2]])) <= horizon[3] + 0.01
    {
        return false;
    }
    let matrix = Mat4::from_cols_array_2d(&frame.render_view.sky_view_projection);
    let clip = matrix * Vec4::new(world.x, world.y, world.z, 0.0);
    if !clip.is_finite() || clip.w <= 0.0 {
        return false;
    }
    let ndc = clip.truncate() / clip.w;
    if ndc.x.abs() >= 0.95 || ndc.y.abs() >= 0.91 {
        return false;
    }
    let width = frame.render_view.viewport[0] as f32;
    let height = frame.render_view.viewport[1] as f32;
    let x = (ndc.x * 0.5 + 0.5) * width + 9.0;
    let y = (0.5 - ndc.y * 0.5) * height - 12.0;
    let text_width = text.len().min(30) as f32 * 8.0;
    if labels.iter().any(|label| {
        (x - label.x).abs() < text_width.max(label.text.len() as f32 * 8.0)
            && (y - label.y).abs() < 18.0
    }) {
        return false;
    }
    labels.push(CelestialLabel { x, y, text });
    true
}
