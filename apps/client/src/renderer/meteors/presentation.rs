//! Adapts bounded fragment histories to the existing producer-neutral frame.
use super::{ActiveEvent, TRAIL_SLOTS_PER_EVENT};
use minimium_renderer::meteors::{
    fragmentation::{material_for_slot, BODY_SLOTS, SOLID_SLOTS},
    model::{MeteorSample, MeteorSource, MeteorTrailSample, TransientLightSample},
    photometry, MeteorFrame, MAX_METEOR_CORES,
};

pub(super) fn push_event(
    frame: &mut MeteorFrame,
    event: &ActiveEvent,
    age_s: f64,
    grains: bool,
) -> u64 {
    let Some(history) = &event.evolution else {
        return 0;
    };
    let range = if grains {
        SOLID_SLOTS..BODY_SLOTS
    } else {
        0..SOLID_SLOTS
    };
    let mut omitted = 0;
    for slot in range {
        if age_s < history.births[slot] {
            continue;
        }
        let body = history.sample_body(age_s, slot);
        if body.gas_power_w <= 0.01 {
            continue;
        }
        if grains && frame.cores().len() >= MAX_METEOR_CORES {
            omitted += 1;
            continue;
        }
        let emission = photometry::emission(
            body,
            material_for_slot(history.material, slot),
            age_s,
            event.seed,
        );
        let id = event.id * BODY_SLOTS as u64 + slot as u64;
        let sample = MeteorSample::new(
            id,
            MeteorSource::Cosmetic,
            event.start_s + history.births[slot],
            body.position_m.to_array(),
            body.velocity_m_s.to_array(),
            emission.rgb,
            emission.intensity,
            emission.radius_m,
            history.phase(age_s, slot),
        );
        if !sample.is_ok_and(|sample| frame.push_core(sample)) {
            omitted += 1;
            continue;
        }
        let begin = (age_s - 0.6).max(history.births[slot]);
        for serial in 0..TRAIL_SLOTS_PER_EVENT {
            let at_s = begin + (age_s - begin) * serial as f64 / TRAIL_SLOTS_PER_EVENT as f64;
            let past = history.sample_body(at_s, slot);
            if let Ok(point) = MeteorTrailSample::new(
                id,
                serial as u32,
                event.start_s + at_s,
                past.position_m.to_array(),
                event.params.trail_thickness_m * if grains { 0.25 } else { 1.0 },
                event.params.trail_opacity,
            ) {
                let _ = frame.push_trail(point);
            }
        }
        if !grains {
            if let Some(light) = event.params.light {
                if let Ok(sample) = TransientLightSample::new(
                    id,
                    body.position_m.to_array(),
                    emission.rgb,
                    emission.intensity,
                    light.range_m,
                    (event.duration_s - age_s).max(0.0),
                ) {
                    let _ = frame.push_light(sample);
                }
            }
        }
    }
    omitted
}
