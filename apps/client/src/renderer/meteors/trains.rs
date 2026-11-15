//! Bounded deposition capture and independent residual-train presentation.
use super::{evolution::Evolution, ActiveEvent, EVENT_ID_STRIDE};
use minimium_renderer::meteors::{
    fragmentation::SOLID_SLOTS,
    model::{MeteorPhase, MeteorSample, MeteorSource, MeteorTrailSample},
    photometry,
    trail::{Deposit, DepositRing, TrailKind, TrailProfile, POINTS},
    MeteorFrame,
};

const DEPOSIT_DT_S: f64 = 0.2;

pub(super) struct Trains {
    pub profile: TrailProfile,
    rings: [DepositRing; SOLID_SLOTS],
    next_index: u32,
    pub skipped_samples: u64,
}

impl Trains {
    pub fn new(seed: u64, large: bool, allow_long: bool) -> Self {
        Self {
            profile: TrailProfile::new(seed, large, allow_long),
            rings: std::array::from_fn(|_| DepositRing::default()),
            next_index: 0,
            skipped_samples: 0,
        }
    }

    pub fn advance(&mut self, history: &Evolution, age_s: f64) {
        let last = (age_s.min(history.duration_s()) / DEPOSIT_DT_S).floor() as u32;
        let begin = self.next_index.max(last.saturating_sub(POINTS as u32 - 1));
        self.skipped_samples += u64::from(begin.saturating_sub(self.next_index));
        for index in begin..=last {
            let at_s = f64::from(index) * DEPOSIT_DT_S;
            for slot in 0..SOLID_SLOTS {
                if at_s < history.births[slot] {
                    continue;
                }
                let body = history.sample_body(at_s, slot);
                if body.gas_power_w <= 0.01 && body.eroded_kg <= 0.0 {
                    continue;
                }
                self.rings[slot].push(Deposit {
                    at_s,
                    position_m: body.position_m,
                    direction: body.velocity_m_s.normalize_or_zero(),
                    visible_energy_j: body.gas_power_w * DEPOSIT_DT_S * 0.02,
                    dust_kg: body.eroded_kg * (DEPOSIT_DT_S / 0.01),
                    serial: index,
                });
            }
        }
        self.next_index = self.next_index.max(last.saturating_add(1));
    }

    pub fn point_count(&self) -> usize {
        self.rings.iter().map(|ring| ring.iter().count()).sum()
    }

    pub fn overwritten(&self) -> u64 {
        self.rings.iter().map(|ring| ring.overwritten).sum()
    }
}

pub(super) fn push(frame: &mut MeteorFrame, event: &ActiveEvent, age_s: f64, radius_m: f64) -> u64 {
    let Some(history) = &event.evolution else {
        return 0;
    };
    let mut omitted = 0;
    for slot in 0..SOLID_SLOTS {
        for kind in [TrailKind::Afterglow, TrailKind::Dust] {
            let ring = &event.trains.rings[slot];
            let profile = &event.trains.profile;
            let count = ring
                .iter()
                .filter(|d| profile.sample(d, age_s, radius_m, kind).is_some())
                .count();
            if count < 2 {
                continue;
            }
            let mut total = 0.0;
            let mut first = None;
            for deposit in ring.iter() {
                if let Some(sample) = profile.sample(deposit, age_s, radius_m, kind) {
                    total += if kind == TrailKind::Dust {
                        sample.opacity * 0.01
                    } else {
                        sample.intensity
                    };
                    first.get_or_insert((deposit, sample));
                }
            }
            let Some((deposit, first)) = first else {
                continue;
            };
            if total <= 0.0 {
                continue;
            }
            let id = event.id * EVENT_ID_STRIDE
                + if kind == TrailKind::Dust { 16 } else { 8 }
                + slot as u64;
            let rgb = photometry::emission(
                history.sample_body(deposit.at_s, slot),
                history.material,
                deposit.at_s,
                event.seed,
            )
            .rgb;
            // Zero radius is a trail-only logical owner, never a visible head.
            let core = MeteorSample::new(
                id,
                MeteorSource::Cosmetic,
                event.start_s,
                first.position_m.to_array(),
                [0.0; 3],
                rgb,
                total,
                0.0,
                MeteorPhase::Fading,
            );
            if !core.is_ok_and(|core| frame.push_core(core)) {
                omitted += 1;
                continue;
            }
            let selected = count.min(super::TRAIL_SLOTS_PER_EVENT);
            let mut ordinal = 0;
            let mut emitted = 0;
            for deposit in ring.iter() {
                let Some(sample) = profile.sample(deposit, age_s, radius_m, kind) else {
                    continue;
                };
                let target = emitted * (count - 1) / (selected - 1);
                let keep = emitted < selected && ordinal == target;
                ordinal += 1;
                if !keep {
                    continue;
                }
                emitted += 1;
                if let Ok(mut point) = MeteorTrailSample::new(
                    id,
                    deposit.serial,
                    event.start_s + deposit.at_s,
                    sample.position_m.to_array(),
                    sample.width_m,
                    sample.opacity,
                ) {
                    point.kind = kind;
                    point.intensity = sample.intensity * count as f64 / selected as f64;
                    if !frame.push_trail(point) {
                        omitted += 1;
                    }
                }
            }
        }
    }
    omitted
}
