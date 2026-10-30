use std::{marker::PhantomData, rc::Rc};

use minimium_core::{sky::ObserverFrame, time::WorldTimeMs};

use crate::{
    ephemeris_cache::{map_status, EphemerisCache},
    ffi,
    frames::{
        body_state, hor_to_world_matrix, matrix_multiply, matrix_vector, normalize,
        validate_observer, validate_rotation, validate_vector, validate_world_transform,
    },
    lunar::{lunar_geometry, LunarGeometryInput},
    model::{
        CorrectedStarDirections, RefractionMode, SkyError, SkySampleContext, SkyState,
        StarAstrometry, EPHEMERIS_REVISION, MOON_MEAN_RADIUS_M, SUN_RADIUS_M,
    },
    stars::StarEpochCache,
    time::ut_days,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SkyInvalidation {
    #[default]
    None,
    Reset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ContextKey {
    clock_revision: minimium_core::time::ClockRevision,
    world_context_key: crate::model::AstronomyContextKey,
    observer_model_revision: u32,
}

pub struct SkyCalculator {
    ephemeris: EphemerisCache,
    star_epoch: StarEpochCache,
    refraction: RefractionMode,
    context: Option<ContextKey>,
    not_send_or_sync: PhantomData<Rc<()>>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SkyCalculatorDiagnostics {
    pub ephemeris_evaluations: u64,
    pub ephemeris_cache_hits: u64,
    pub star_epoch_updates: u64,
    pub star_epoch_cache_hits: u64,
}

impl Default for SkyCalculator {
    fn default() -> Self {
        Self::new(RefractionMode::Standard)
    }
}

impl SkyCalculator {
    pub fn new(refraction: RefractionMode) -> Self {
        Self {
            ephemeris: EphemerisCache::default(),
            star_epoch: StarEpochCache::default(),
            refraction,
            context: None,
            not_send_or_sync: PhantomData,
        }
    }

    pub fn evaluate(
        &mut self,
        sample: SkySampleContext,
        observer: ObserverFrame,
        invalidation: SkyInvalidation,
    ) -> Result<SkyState, SkyError> {
        validate_observer(&observer, sample.observer_model_revision)?;
        let key = ContextKey {
            clock_revision: sample.clock_revision,
            world_context_key: sample.world_context_key,
            observer_model_revision: sample.observer_model_revision,
        };
        let reset = invalidation == SkyInvalidation::Reset || self.context != Some(key);
        if reset {
            self.ephemeris.invalidate();
            self.star_epoch.invalidate();
        }
        self.context = Some(key);

        let values = self.ephemeris.sample(sample, reset)?;
        let ut_days = ut_days(sample.utc_ms)?;
        let local = ffi::local_frame(
            ut_days,
            observer.geographic.latitude_rad().to_degrees(),
            observer.geographic.longitude_rad().to_degrees(),
            observer.geographic.height_m(),
        )
        .map_err(map_status)?;
        if !local.tt_days.is_finite() {
            return Err(SkyError::NonFinite);
        }
        let eqj_to_hor = validate_rotation(local.eqj_to_hor)?;
        let hor_to_world = hor_to_world_matrix(&observer);
        let eqj_to_world = validate_world_transform(matrix_multiply(hor_to_world, eqj_to_hor))?;
        let observer_eqj = validate_vector(local.observer_geocentric_eqj_m)?;
        let sun_topocentric = subtract(values.sun_geocentric_m, observer_eqj);
        let moon_topocentric = subtract(values.moon_geocentric_m, observer_eqj);
        let sun_distance_m = crate::frames::length(sun_topocentric);
        let moon_distance_m = crate::frames::length(moon_topocentric);
        if !sun_distance_m.is_finite() || !moon_distance_m.is_finite() {
            return Err(SkyError::NonFinite);
        }
        let sun = body_state(
            sun_topocentric,
            sun_distance_m,
            SUN_RADIUS_M,
            eqj_to_hor,
            eqj_to_world,
            &observer,
            self.refraction,
        )?;
        let moon_body = body_state(
            moon_topocentric,
            moon_distance_m,
            MOON_MEAN_RADIUS_M,
            eqj_to_hor,
            eqj_to_world,
            &observer,
            self.refraction,
        )?;
        let moon = lunar_geometry(LunarGeometryInput {
            moon_body,
            moon_geocentric_eqj_m: values.moon_geocentric_m,
            earth_heliocentric_eqj_m: values.earth_heliocentric_m,
            observer_geocentric_eqj_m: observer_eqj,
            moon_north_eqj: values.moon_north_eqj,
            ecliptic_phase_rad: values.ecliptic_phase_rad,
            apparent_v_magnitude: values.moon_v_magnitude,
            eqj_to_world,
        })?;
        let geocentric_sun_direction_world = normalize(matrix_vector(
            eqj_to_world,
            normalize(values.sun_geocentric_m)?,
        ))?;
        Ok(SkyState {
            utc_ms: sample.utc_ms,
            observer_model_revision: sample.observer_model_revision,
            sun,
            moon,
            geocentric_sun_direction_world,
            catalog_epoch_to_world: eqj_to_world,
            ephemeris_revision: EPHEMERIS_REVISION,
        })
    }

    pub fn update_star_epoch(
        &mut self,
        utc_ms: WorldTimeMs,
        records: &[StarAstrometry],
    ) -> Result<CorrectedStarDirections, SkyError> {
        if !self.star_epoch.needs_update(utc_ms, records)? {
            if let Some(cached) = self.star_epoch.cached(utc_ms) {
                return Ok(cached);
            }
        }
        let terrestrial_days = ffi::terrestrial_days(ut_days(utc_ms)?).map_err(map_status)?;
        self.star_epoch.update(utc_ms, terrestrial_days, records)
    }

    pub fn diagnostics(&self) -> SkyCalculatorDiagnostics {
        let (ephemeris_evaluations, ephemeris_cache_hits) = self.ephemeris.diagnostics();
        let (star_epoch_updates, star_epoch_cache_hits) = self.star_epoch.diagnostics();
        SkyCalculatorDiagnostics {
            ephemeris_evaluations,
            ephemeris_cache_hits,
            star_epoch_updates,
            star_epoch_cache_hits,
        }
    }

    pub fn invalidate_star_epoch(&mut self) {
        self.star_epoch.invalidate();
    }
}

fn subtract(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| a[axis] - b[axis])
}
