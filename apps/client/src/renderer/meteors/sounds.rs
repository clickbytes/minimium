//! Rare breakup cues; histories remain independent of optional playback.
use super::ActiveEvent;
use crate::{
    audio::{
        meteors::{CueKind, MeteorSoundCue},
        MeteorAudio,
    },
    renderer::environment::RenderViewFrame,
};
use glam::DVec3;
use minimium_renderer::meteors::{coords::PlanetSurfaceScale, fragmentation::EventKind};

pub(super) fn listener(view: &RenderViewFrame, scale: PlanetSurfaceScale) -> [f64; 3] {
    let eye = DVec3::from_array(view.camera_eye_world_m);
    (eye.normalize_or_zero()
        * (view.game_planet_radius_m
            + scale.physical_altitude_m(eye.length() - view.game_planet_radius_m)))
    .to_array()
}
pub(super) fn capture(
    event: &mut ActiveEvent,
    audio: &mut MeteorAudio,
    view: &RenderViewFrame,
    scale: PlanetSurfaceScale,
    now_s: f64,
) {
    if event.params.mass_kg < 20.0 {
        return;
    }
    let Some(history) = &event.evolution else {
        return;
    };
    for fracture in &history.events[..history.event_count] {
        if !matches!(fracture.kind, EventKind::Breakup)
            || fracture.slot >= 4
            || event.start_s + fracture.at_s > now_s
        {
            continue;
        }
        let bit = 1u8 << fracture.slot;
        if event.sound_slots & bit != 0 {
            continue;
        }
        let body = history.sample_body(fracture.at_s, fracture.slot as usize);
        let energy = 0.5 * fracture.mass_kg * body.velocity_m_s.length_squared();
        if energy < 1e10 {
            continue;
        }
        event.sound_slots |= bit;
        let right = [
            f64::from(view.view[0][0]),
            f64::from(view.view[1][0]),
            f64::from(view.view[2][0]),
        ];
        audio.enqueue(
            MeteorSoundCue {
                source_pos: body.position_m.to_array(),
                onset_monotonic: event.start_s + fracture.at_s,
                relative_energy: (energy / 1e11).clamp(0.0, 1.0) as f32,
                cue_kind: CueKind::Airburst,
                seed: event.seed ^ u64::from(fracture.slot),
            },
            listener(view, scale),
            right,
            now_s,
        );
    }
}
