use std::collections::HashSet;
use std::sync::Arc;

use minimium_core::time::WorldTimeMs;

use crate::{
    frames::normalize,
    model::{CorrectedStarDirections, SkyError, StarAstrometry, StarDirection, MAX_STAR_RECORDS},
    time::JULIAN_YEAR_DAYS,
};

const STAR_DAY_MS: i64 = 86_400_000;

#[derive(Default)]
pub(crate) struct StarEpochCache {
    last_utc_day: Option<i64>,
    last_utc_ms: Option<i64>,
    records: Vec<StarAstrometry>,
    result: Option<CorrectedStarDirections>,
    invalidated: bool,
    updates: u64,
    cache_hits: u64,
}

impl StarEpochCache {
    pub fn diagnostics(&self) -> (u64, u64) {
        (self.updates, self.cache_hits)
    }

    pub fn invalidate(&mut self) {
        self.invalidated = true;
    }

    pub fn needs_update(
        &self,
        utc_ms: WorldTimeMs,
        records: &[StarAstrometry],
    ) -> Result<bool, SkyError> {
        let records_changed = self.records.as_slice() != records;
        if records_changed || self.result.is_none() || self.invalidated {
            validate_records(records)?;
        }
        let day = utc_ms.get().div_euclid(STAR_DAY_MS);
        let discontinuity = self.last_utc_ms.is_some_and(|previous| {
            let current = utc_ms.get();
            current < previous || current.saturating_sub(previous) > STAR_DAY_MS
        });
        Ok(self.invalidated
            || self.result.is_none()
            || self.last_utc_day != Some(day)
            || discontinuity
            || records_changed)
    }

    pub fn cached(&mut self, utc_ms: WorldTimeMs) -> Option<CorrectedStarDirections> {
        if self.result.is_some() {
            self.cache_hits = self.cache_hits.saturating_add(1);
        }
        self.last_utc_day = Some(utc_ms.get().div_euclid(STAR_DAY_MS));
        self.last_utc_ms = Some(utc_ms.get());
        self.result.clone()
    }

    pub fn update(
        &mut self,
        utc_ms: WorldTimeMs,
        tt_days: f64,
        records: &[StarAstrometry],
    ) -> Result<CorrectedStarDirections, SkyError> {
        validate_records(records)?;
        if !tt_days.is_finite() {
            return Err(SkyError::NonFinite);
        }
        let delta_jyear = tt_days / JULIAN_YEAR_DAYS;
        if !delta_jyear.is_finite() {
            return Err(SkyError::NonFinite);
        }
        let mut directions = Vec::with_capacity(records.len());
        for star in records {
            let position = std::array::from_fn(|axis| {
                star.position_eqj_pc[axis] + star.velocity_eqj_pc_per_jyear[axis] * delta_jyear
            });
            directions.push(StarDirection {
                id: star.id,
                direction_eqj: normalize(position)?,
            });
        }
        let revision = self.result.as_ref().map_or(Ok(1), |previous| {
            previous
                .star_epoch_revision
                .checked_add(1)
                .ok_or(SkyError::RevisionExhausted)
        })?;
        let result = CorrectedStarDirections {
            star_epoch_revision: revision,
            directions: Arc::from(directions),
        };
        self.records.clear();
        self.records.extend_from_slice(records);
        self.last_utc_day = Some(utc_ms.get().div_euclid(STAR_DAY_MS));
        self.last_utc_ms = Some(utc_ms.get());
        self.invalidated = false;
        self.result = Some(result.clone());
        self.updates = self.updates.saturating_add(1);
        Ok(result)
    }
}

fn validate_records(records: &[StarAstrometry]) -> Result<(), SkyError> {
    if records.len() > MAX_STAR_RECORDS {
        return Err(SkyError::InvalidStarData);
    }
    let mut ids = HashSet::with_capacity(records.len());
    for star in records {
        if star.id == 0
            || !ids.insert(star.id)
            || !star.position_eqj_pc.iter().all(|value| value.is_finite())
            || !star
                .velocity_eqj_pc_per_jyear
                .iter()
                .all(|value| value.is_finite())
        {
            return Err(SkyError::InvalidStarData);
        }
        normalize(star.position_eqj_pc).map_err(|_| SkyError::InvalidStarData)?;
    }
    Ok(())
}
