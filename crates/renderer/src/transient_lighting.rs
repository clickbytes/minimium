//! Bounded local point lights, independent of producer, protocol and GPU effects.
use crate::meteors::{MeteorFrame, MeteorFrameContext, MeteorFrameState, TransientLightMoment};
use bytemuck::{Pod, Zeroable};
use glam::DVec3;

pub const CAPACITY: usize = 4;
/// Scene-linear RGB radiant intensity already contains the isotropic 1/(4 pi).
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct TransientLight {
    pub position_radius: [f32; 4],
    pub rgb_intensity: [f32; 4],
    /// Physical range, remaining seconds; reserved components are zero.
    pub range_lifetime: [f32; 4],
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct TransientLightUniform {
    pub count: [u32; 4],
    /// Planet centre relative to render origin, game radius.
    pub planet: [f32; 4],
    /// Physical altitude at game sea level, physical metres per radial game metre.
    pub mapping: [f32; 4],
    pub sources: [TransientLight; CAPACITY],
}

fn physical(world: DVec3, context: MeteorFrameContext) -> DVec3 {
    let length = world.length();
    world.normalize_or_zero()
        * (context.game_radius_m
            + context
                .surface_scale
                .physical_altitude_m(length - context.game_radius_m))
}

/// Finite local sources only. Ranking is physical received power, luminance
/// weighted, with a smooth final 20% range fade. The caller breaks ties by ID.
pub fn score(light: &TransientLightMoment, context: MeteorFrameContext) -> f64 {
    if !light.is_finite_contract() || light.ttl_s <= 0.0 || light.range_m <= 0.0 {
        return 0.0;
    }
    let source = DVec3::from_array(context.render_origin_m)
        + DVec3::from_array(light.position_rel_m.map(f64::from));
    let altitude = context
        .surface_scale
        .physical_altitude_m(source.length() - context.game_radius_m);
    if !(0.0..=1_000_000.0).contains(&altitude) {
        return 0.0;
    }
    let eye = DVec3::from_array(context.camera_eye_world_m);
    if !crate::meteors::visibility::head_visible(
        eye.to_array(),
        source.to_array(),
        context.game_radius_m,
        0.0,
    ) {
        return 0.0;
    }
    let distance = physical(source, context).distance(physical(eye, context));
    let fade = range_fade(distance, f64::from(light.range_m));
    let luminance = f64::from(light.linear_rgb[0]) * 0.2126
        + f64::from(light.linear_rgb[1]) * 0.7152
        + f64::from(light.linear_rgb[2]) * 0.0722;
    light.luminous_intensity * luminance * fade / distance.powi(2).max(1.0)
}

pub fn range_fade(distance: f64, range: f64) -> f64 {
    let x = ((range - distance) / (range * 0.2).max(1e-6)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

impl TransientLightUniform {
    pub const BYTES: u64 = std::mem::size_of::<Self>() as u64;

    pub fn from_frame(frame: &MeteorFrame) -> Self {
        if frame.state() != MeteorFrameState::Ready {
            return Self::default();
        }
        let context = frame.context();
        let anchor = context.surface_scale.physical_altitude_m(0.0);
        let inverse_scale = context.surface_scale.physical_altitude_m(1.0) - anchor;
        let mut result = Self {
            planet: [
                -context.render_origin_m[0] as f32,
                -context.render_origin_m[1] as f32,
                -context.render_origin_m[2] as f32,
                context.game_radius_m as f32,
            ],
            mapping: [anchor as f32, inverse_scale as f32, 0.0, 0.0],
            ..Self::default()
        };
        for light in frame.lights() {
            if score(light, context) <= 0.0 {
                continue;
            }
            let radius = frame
                .cores()
                .iter()
                .find(|core| core.id == light.id)
                .map_or(1.0, |core| core.radius_m)
                .max(0.01);
            result.sources[result.count[0] as usize] = TransientLight {
                position_radius: [
                    light.position_rel_m[0],
                    light.position_rel_m[1],
                    light.position_rel_m[2],
                    radius,
                ],
                rgb_intensity: [
                    light.linear_rgb[0],
                    light.linear_rgb[1],
                    light.linear_rgb[2],
                    light.luminous_intensity as f32,
                ],
                range_lifetime: [light.range_m, light.ttl_s, 0.0, 0.0],
            };
            result.count[0] += 1;
            if result.count[0] as usize == CAPACITY {
                break;
            }
        }
        result
    }

    /// Read-only observer diagnostic; no exposure or global ambient modification.
    pub fn observer_irradiance(frame: &MeteorFrame) -> f64 {
        frame
            .lights()
            .iter()
            .map(|light| score(light, frame.context()))
            .sum()
    }
}

const _: () = {
    assert!(std::mem::size_of::<TransientLight>() == 48);
    assert!(std::mem::offset_of!(TransientLight, rgb_intensity) == 16);
    assert!(std::mem::offset_of!(TransientLight, range_lifetime) == 32);
    assert!(std::mem::size_of::<TransientLightUniform>() == 240);
    assert!(std::mem::align_of::<TransientLightUniform>() == 16);
    assert!(std::mem::offset_of!(TransientLightUniform, planet) == 16);
    assert!(std::mem::offset_of!(TransientLightUniform, mapping) == 32);
    assert!(std::mem::offset_of!(TransientLightUniform, sources) == 48);
};
