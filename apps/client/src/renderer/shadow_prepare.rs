use super::{
    Renderer,
    environment::{EnvironmentFrame, EnvironmentFrameStatus},
    shadow_map_cache::{PreparedMapCasters, ShadowMapPreparationCache},
    shadows::{self, ShadowInvalidReason, ShadowState, ShadowUniform},
};
use minimium_protocol::WorldContext;
use minimium_world::world::{
    GenerationRevision, SectorKey, WorldContentRevision, WorldInstanceId, WorldMap,
};

pub(super) struct ShadowSectorSnapshot {
    pub(super) key: SectorKey,
    pub(super) lod: u8,
    pub(super) resident: bool,
    pub(super) bounds: Option<([f64; 3], [f64; 3])>,
}

pub(super) struct PreparedShadowFrame {
    pub(super) context: Option<WorldContext>,
    pub(super) instance: Option<WorldInstanceId>,
    pub(super) generation: Option<GenerationRevision>,
    pub(super) content_revision: Option<WorldContentRevision>,
    pub(super) geometry_stamp:
        Option<minimium_renderer::world_geometry_gpu::GeometryPublicationStamp>,
    pub(super) caster_keys: Vec<(SectorKey, u8)>,
    pub(super) uniform: ShadowUniform,
    pub(super) map_casters: Option<PreparedMapCasters>,
    pub(super) invalid_reason: Option<ShadowInvalidReason>,
    pub(super) valid: bool,
}

impl PreparedShadowFrame {
    pub(super) fn invalid(reason: ShadowInvalidReason) -> Self {
        Self {
            context: None,
            instance: None,
            generation: None,
            content_revision: None,
            geometry_stamp: None,
            caster_keys: Vec::new(),
            uniform: bytemuck::Zeroable::zeroed(),
            map_casters: None,
            invalid_reason: Some(reason),
            valid: false,
        }
    }
}

pub(super) struct ShadowPreparationInput<'a> {
    pub(super) environment: &'a EnvironmentFrame,
    pub(super) instance: WorldInstanceId,
    pub(super) map: Option<&'a WorldMap>,
    pub(super) geometry_version: Option<(GenerationRevision, WorldContentRevision)>,
    pub(super) geometry_stamp:
        Option<minimium_renderer::world_geometry_gpu::GeometryPublicationStamp>,
    pub(super) active_sectors: &'a [ShadowSectorSnapshot],
    pub(super) scene_visible: bool,
    pub(super) overview_active: bool,
    pub(super) overview_planet: bool,
    pub(super) remote_complete: bool,
    pub(super) shadow_enabled: bool,
    pub(super) physical: bool,
    pub(super) settings: minimium_renderer::ShadowSettings,
    pub(super) player_position_m: [f64; 3],
    pub(super) render_origin: [f32; 3],
    pub(super) atmosphere: &'a minimium_renderer::AtmosphereUniform,
    pub(super) bolid_light: Option<([f32; 4], [f32; 4])>,
}

pub(super) fn prepare_shadow_frame(
    input: ShadowPreparationInput<'_>,
    state: &mut ShadowState,
    map_cache: &mut ShadowMapPreparationCache,
) -> Result<PreparedShadowFrame, ShadowInvalidReason> {
    state.caster_keys.clear();
    state.valid = false;
    state.source = None;
    state.invalid_reason = None;
    state.receiver_radius_m = 0.0;
    state.texel_m = 0.0;
    if !input.shadow_enabled {
        return Err(ShadowInvalidReason::Disabled);
    }
    if input.environment.status != EnvironmentFrameStatus::Available
        || !input.scene_visible
        || (input.overview_active && !input.overview_planet)
    {
        return Err(ShadowInvalidReason::InvalidFrame);
    }
    let Some(context) = input.environment.context else {
        return Err(ShadowInvalidReason::StaleContext);
    };
    let Some((generation, content_revision)) = input.geometry_version else {
        return Err(ShadowInvalidReason::StaleContext);
    };
    let Some(geometry_stamp) = input.geometry_stamp else {
        return Err(ShadowInvalidReason::StaleContext);
    };
    if geometry_stamp.instance != input.instance
        || geometry_stamp.generation != generation
        || geometry_stamp.content_revision != content_revision
    {
        return Err(ShadowInvalidReason::StaleContext);
    }
    if context.instance() != input.instance || content_revision != context.content_revision() {
        return Err(ShadowInvalidReason::StaleContext);
    }
    let Some(map) = input.map else {
        return Err(ShadowInvalidReason::StaleContext);
    };
    if map.fingerprint() != geometry_stamp.world_fingerprint {
        return Err(ShadowInvalidReason::StaleContext);
    }
    let ground_distance = input
        .active_sectors
        .iter()
        .filter_map(|sector| sector.bounds)
        .map(|(minimum, maximum)| {
            bounds_distance(
                minimum,
                maximum,
                input.environment.render_view.camera_eye_world_m,
            )
        })
        .fold(f64::INFINITY, f64::min);
    let Some(region) = minimium_renderer::shadow_view::ShadowRegion::for_view(
        input.environment.render_view,
        input.player_position_m,
        ground_distance,
        input.settings,
        state.receiver_radius_m,
    ) else {
        return Err(ShadowInvalidReason::IncompleteCasters);
    };
    state.receiver_radius_m = region.settings.receiver_radius_m;
    state.texel_m =
        region.settings.receiver_radius_m * 2.02 / region.settings.resolution.max(1) as f32;
    let physical = input.physical;
    let bolid_position = input.bolid_light.and_then(|(position, color)| {
        let distance_squared = (0..3)
            .map(|i| (f64::from(position[i]) - region.center_world[i]).powi(2))
            .sum::<f64>();
        let irradiance = minimium_renderer::bolid_light::incident_luminance(
            position,
            color,
            region.center_world,
            input.atmosphere,
        );
        (irradiance > 0.000025
            && distance_squared < 160_000.0_f64.powi(2)
            && input
                .environment
                .environment_uniform
                .sky_zenith_rgb_sun_altitude[3]
                < -0.12)
            .then(|| {
                // A bounded single-map takeover must not introduce full
                // occlusion at the first frame crossing its eligibility gate.
                let ramp = |value: f64, low: f64, high: f64| {
                    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                };
                let night = -f64::from(
                    input
                        .environment
                        .environment_uniform
                        .sky_zenith_rgb_sun_altitude[3],
                );
                let weight = ramp(irradiance, 0.000025, 0.000075)
                    * (1.0 - ramp(distance_squared.sqrt(), 140_000.0, 160_000.0))
                    * ramp(night, 0.12, 0.18);
                (position, weight as f32)
            })
    });
    let Some(source) = bolid_position
        .map(|_| shadows::ShadowSource::Bolid)
        .or_else(|| {
            shadows::select_source(
                input.environment,
                state.previous_source,
                region.center_world,
                physical,
            )
        })
    else {
        state.previous_source = None;
        return Err(ShadowInvalidReason::NoSource);
    };
    state.previous_source = Some(source);
    state.source = Some(source);
    if (!input.overview_active && !input.remote_complete) || input.active_sectors.is_empty() {
        return Err(ShadowInvalidReason::IncompleteCasters);
    }
    let radius = f64::from(region.settings.caster_radius_m);
    for sector in input.active_sectors {
        let Some((minimum, maximum)) = sector.bounds else {
            return Err(ShadowInvalidReason::IncompleteCasters);
        };
        if !sector.resident {
            return Err(ShadowInvalidReason::IncompleteCasters);
        }
        if bounds_intersect_sphere(minimum, maximum, region.center_world, radius) {
            state.caster_keys.push((sector.key, sector.lod));
        }
    }
    if state.caster_keys.is_empty() {
        return Err(ShadowInvalidReason::IncompleteCasters);
    }
    let direction = if let Some((position, _)) = bolid_position {
        let delta = std::array::from_fn(|i| f64::from(position[i]) - region.center_world[i]);
        let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
        (length > 1.0).then(|| delta.map(|v| v / length))
    } else {
        source.direction(input.environment, physical)
    };
    let Some(direction) = direction else {
        return Err(ShadowInvalidReason::InvalidFrame);
    };
    let uniform = if let Some((position, _)) = bolid_position {
        minimium_renderer::shadow_uniform::prepare_point_at_origin(
            [
                f64::from(position[0]),
                f64::from(position[1]),
                f64::from(position[2]),
            ],
            region.center_world,
            input.render_origin.map(f64::from),
            region.settings,
        )
    } else {
        minimium_renderer::shadow_uniform::prepare_at_origin(
            direction,
            region.center_world,
            input.render_origin.map(f64::from),
            region.settings,
            source.id(),
        )
    };
    let Some(mut uniform) = uniform else {
        return Err(ShadowInvalidReason::InvalidFrame);
    };
    if let Some((_, weight)) = bolid_position {
        uniform.flags[2] = weight.to_bits();
    }
    let map_casters = map_cache
        .prepare(
            geometry_stamp,
            input.instance,
            generation,
            content_revision,
            map,
            region.center_world,
            region.settings.caster_radius_m,
            input.render_origin,
        )
        .map_err(|()| ShadowInvalidReason::BudgetExceeded)?;
    state.valid = true;
    state.invalid_reason = None;
    Ok(PreparedShadowFrame {
        context: Some(context),
        instance: Some(input.instance),
        generation: Some(generation),
        content_revision: Some(content_revision),
        geometry_stamp: Some(geometry_stamp),
        caster_keys: state.caster_keys.clone(),
        uniform,
        map_casters: Some(map_casters),
        invalid_reason: None,
        valid: true,
    })
}

impl Renderer {
    pub(super) fn prepare_shadow_frame(
        &mut self,
        environment: &EnvironmentFrame,
        player_position_m: [f64; 3],
    ) {
        let instance = self.application.game.current_instance;
        let publication = self.gpu.world_geometry.publication().cloned();
        let active_sectors = publication
            .as_ref()
            .into_iter()
            .flat_map(|publication| publication.keys.iter())
            .map(|(key, lod)| {
                let catalog_key = (instance, *key, *lod);
                let bounds = self
                    .gpu
                    .world_geometry
                    .bounds()
                    .get(&catalog_key)
                    .map(|bounds| (bounds.minimum, bounds.maximum));
                ShadowSectorSnapshot {
                    key: *key,
                    lod: *lod,
                    resident: self.gpu.world_geometry.contains(instance, *key, *lod),
                    bounds,
                }
            })
            .collect::<Vec<_>>();
        let geometry_version = self.gpu.world_geometry.versions().get(&instance).copied();
        let geometry_stamp = publication.map(|publication| publication.stamp);
        let input = ShadowPreparationInput {
            environment,
            instance,
            map: self.application.world_catalog.get(&instance),
            geometry_version,
            geometry_stamp,
            active_sectors: &active_sectors,
            scene_visible: self.gpu.scene_visible,
            overview_active: self.gpu.overview_active,
            overview_planet: self.gpu.overview_planet,
            remote_complete: self.gpu.shadow_remote_complete
                && self.gpu.shadow_remote_stamp == geometry_stamp,
            shadow_enabled: self.gpu.shadow_gpu.settings.enabled,
            physical: self.gpu.atmosphere_gpu.is_active(),
            settings: self.gpu.shadow_gpu.settings,
            player_position_m,
            render_origin: self.gpu.render_origin,
            atmosphere: self.gpu.atmosphere_gpu.optical_uniform(),
            bolid_light: super::bolids::strongest_bolid_light(
                &self.prepared_bolids,
                environment.render_view.camera_eye_world_m,
                self.gpu.atmosphere_gpu.optical_uniform(),
            ),
        };
        match prepare_shadow_frame(
            input,
            &mut self.shadow_state,
            &mut self.shadow_map_preparation,
        ) {
            Ok(prepared) => {
                self.prepared_shadow = prepared;
            }
            Err(reason) => {
                self.shadow_state.valid = false;
                self.shadow_state.caster_keys.clear();
                self.shadow_state.invalid_reason = Some(reason);
                self.prepared_shadow = PreparedShadowFrame::invalid(reason);
            }
        }
    }
}

fn bounds_distance(minimum: [f64; 3], maximum: [f64; 3], point: [f64; 3]) -> f64 {
    (0..3)
        .map(|axis| {
            (minimum[axis] - point[axis])
                .max(point[axis] - maximum[axis])
                .max(0.0)
                .powi(2)
        })
        .sum::<f64>()
        .sqrt()
}

fn bounds_intersect_sphere(
    minimum: [f64; 3],
    maximum: [f64; 3],
    center: [f64; 3],
    radius: f64,
) -> bool {
    let distance_squared = (0..3)
        .map(|axis| {
            let closest = center[axis].clamp(minimum[axis], maximum[axis]);
            (center[axis] - closest).powi(2)
        })
        .sum::<f64>();
    distance_squared <= radius * radius
}
