use super::{
    environment::{EnvironmentFrame, EnvironmentFrameStatus},
    shadow_map_cache::{PreparedMapCasters, ShadowMapPreparationCache},
    shadows::{self, ShadowInvalidReason, ShadowState, ShadowUniform},
    DesktopHost,
};
use minimium_protocol::WorldContext;
use minimium_renderer::{shadow_geometry::ShadowBounds, shadow_view::ShadowRegion};
use minimium_world::world::{
    GenerationRevision, SectorKey, WorldContentRevision, WorldInstanceId, WorldMap,
};

pub(super) struct ShadowSectorSnapshot {
    pub(super) key: SectorKey,
    pub(super) lod: u8,
    pub(super) vertex_count: Option<u32>,
    pub(super) bounds: Option<ShadowBounds>,
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
    pub(super) secondary_moon: [Option<ShadowUniform>; 2],
    pub(super) moon_keys: [Vec<(SectorKey, u8)>; 2],
    pub(super) landscape_uniform: Option<ShadowUniform>,
    pub(super) landscape_caster_keys: Vec<(SectorKey, u8)>,
    pub(super) map_casters: Option<PreparedMapCasters>,
    pub(super) valid: bool,
}

impl PreparedShadowFrame {
    pub(super) fn invalid() -> Self {
        Self {
            context: None,
            instance: None,
            generation: None,
            content_revision: None,
            geometry_stamp: None,
            caster_keys: Vec::new(),
            uniform: bytemuck::Zeroable::zeroed(),
            secondary_moon: [None; 2],
            moon_keys: Default::default(),
            landscape_uniform: None,
            landscape_caster_keys: Vec::new(),
            map_casters: None,
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
    let previous_radius = if state.valid {
        state.receiver_radius_m
    } else {
        0.0
    };
    state.caster_keys.clear();
    state.valid = false;
    state.source = None;
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
        .map(|bounds| bounds.distance_to(input.environment.render_view.camera_eye_world_m))
        .fold(f64::INFINITY, f64::min);
    let Some(region) = minimium_renderer::shadow_view::ShadowRegion::for_view(
        input.environment.render_view,
        input.player_position_m,
        ground_distance,
        input.settings,
        previous_radius,
    ) else {
        return Err(ShadowInvalidReason::IncompleteCasters);
    };
    state.receiver_radius_m = region.settings.receiver_radius_m;
    state.texel_m =
        region.settings.receiver_radius_m * 2.02 / region.settings.resolution.max(1) as f32;
    let atmosphere = (input.atmosphere.config[0] > 0.0).then_some(input.atmosphere);
    let origin = input.render_origin.map(f64::from);
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
    // A point inside the receiver region cannot provide a perspective shadow.
    // Keep the directional maps eligible when that projection cannot be built.
    let bolid_shadow = bolid_position.and_then(|(position, weight)| {
        let source = [position[0], position[1], position[2]].map(f64::from);
        let mut uniform = minimium_renderer::shadow_uniform::prepare_point_at_origin(
            source,
            region.center_world,
            origin,
            region.settings,
        )?;
        uniform.flags[2] = weight.to_bits();
        Some(uniform)
    });
    let (primary, footprint_moonlight) = shadows::select_source(
        input.environment,
        state.previous_source,
        region.center_world,
        atmosphere,
    );
    let moon = shadows::ShadowSource::Moon;
    let lunar_landscape = region.lunar_landscape(input.environment.render_view);
    let moon_contributes = footprint_moonlight
        || shadows::published_moonlight(
            input.environment,
            atmosphere,
            lunar_landscape.unwrap_or(region),
            input.active_sectors.iter().filter_map(|s| s.bounds),
        );
    let Some(source) = bolid_shadow
        .map(|_| shadows::ShadowSource::Bolid)
        .or(primary)
        .or_else(|| moon_contributes.then_some(moon))
    else {
        state.previous_source = None;
        return Err(ShadowInvalidReason::NoSource);
    };
    state.previous_source = Some(source);
    state.source = Some(source);
    if (!input.overview_active && !input.remote_complete) || input.active_sectors.is_empty() {
        return Err(ShadowInvalidReason::IncompleteCasters);
    }
    // An empty, resident patch (for example ocean without a solid caster) is
    // a complete publication. A missing buffer or bounds on a nonempty one is not.
    if input.active_sectors.iter().any(|sector| {
        sector.vertex_count.is_none() || (sector.vertex_count != Some(0) && sector.bounds.is_none())
    }) {
        return Err(ShadowInvalidReason::IncompleteCasters);
    }
    let mut landscape_uniform = None;
    let mut landscape_caster_keys = Vec::new();
    let [mx, my, mz, mr] = input.environment.environment_uniform.moon_orbit_world;
    let moon_distance =
        glam::DVec3::new(mx.into(), my.into(), mz.into()).distance(region.center_world.into());
    let lunar_radius = (f64::from(mr) / moon_distance.max(1.0))
        .clamp(0.0, 1.0)
        .asin();
    let mut moon_keys = [Vec::new(), Vec::new()];
    let mut secondary_moon = [None; 2];
    if source != moon && moon_contributes {
        let direction = moon
            .direction(input.environment, atmosphere, region.center_world.into())
            .ok_or(ShadowInvalidReason::InvalidFrame)?
            .to_array();
        for (i, area) in [Some(region), lunar_landscape].into_iter().enumerate() {
            if let Some(area) = area {
                secondary_moon[i] = directional_cascade(
                    area,
                    direction,
                    origin,
                    moon.id(),
                    lunar_radius,
                    input.active_sectors,
                    &mut moon_keys[i],
                );
            }
        }
    }
    let uniform = if let Some(point) = bolid_shadow {
        let radius = f64::from(region.settings.caster_radius_m);
        state
            .caster_keys
            .extend(input.active_sectors.iter().filter_map(|sector| {
                (sector.vertex_count != Some(0)
                    && sector
                        .bounds?
                        .intersects_sphere(region.center_world, radius))
                .then_some((sector.key, sector.lod))
            }));
        Some(point)
    } else {
        let direction = source
            .direction(input.environment, atmosphere, region.center_world.into())
            .ok_or(ShadowInvalidReason::InvalidFrame)?
            .to_array();
        let near = directional_cascade(
            region,
            direction,
            origin,
            source.id(),
            lunar_radius,
            input.active_sectors,
            &mut state.caster_keys,
        );
        let landscape = if source == moon {
            lunar_landscape
        } else {
            region.landscape(input.environment.render_view)
        };
        if let Some(landscape) = landscape {
            landscape_uniform = directional_cascade(
                landscape,
                direction,
                origin,
                source.id(),
                lunar_radius,
                input.active_sectors,
                &mut landscape_caster_keys,
            );
        }
        near
    };
    let Some(uniform) = uniform else {
        return Err(ShadowInvalidReason::InvalidFrame);
    };
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
    Ok(PreparedShadowFrame {
        context: Some(context),
        instance: Some(input.instance),
        generation: Some(generation),
        content_revision: Some(content_revision),
        geometry_stamp: Some(geometry_stamp),
        caster_keys: state.caster_keys.clone(),
        uniform,
        secondary_moon,
        moon_keys,
        landscape_uniform,
        landscape_caster_keys,
        map_casters: Some(map_casters),
        valid: true,
    })
}

impl DesktopHost {
    pub(super) fn prepare_shadow_frame(
        &mut self,
        environment: &EnvironmentFrame,
        player_position_m: [f64; 3],
    ) {
        let instance = self.application.core_view().world_instance;
        let publication = self.renderer.world_geometry.publication().cloned();
        let active_sectors = publication
            .as_ref()
            .into_iter()
            .flat_map(|publication| publication.keys.iter())
            .map(|(key, lod)| {
                let catalog_key = (instance, *key, *lod);
                let bounds = self
                    .renderer
                    .world_geometry
                    .bounds()
                    .get(&catalog_key)
                    .copied();
                ShadowSectorSnapshot {
                    key: *key,
                    lod: *lod,
                    vertex_count: self
                        .renderer
                        .world_geometry
                        .catalog()
                        .get(&catalog_key)
                        .map(|(_, count)| *count),
                    bounds,
                }
            })
            .collect::<Vec<_>>();
        let geometry_version = self
            .renderer
            .world_geometry
            .versions()
            .get(&instance)
            .copied();
        let geometry_stamp = publication.map(|publication| publication.stamp);
        let input = ShadowPreparationInput {
            environment,
            instance,
            map: self.application.world_catalog_view().get(&instance),
            geometry_version,
            geometry_stamp,
            active_sectors: &active_sectors,
            scene_visible: self.renderer.scene_visible,
            overview_active: self.renderer.overview_active,
            overview_planet: self.renderer.overview_planet,
            remote_complete: self.renderer.shadow_remote_complete
                && self.renderer.shadow_remote_stamp == geometry_stamp,
            shadow_enabled: self.renderer.shadow_gpu.settings.enabled,
            settings: self.renderer.shadow_gpu.settings,
            player_position_m,
            render_origin: self.renderer.render_origin,
            atmosphere: self.renderer.atmosphere_gpu.optical_uniform(),
            bolid_light: super::bolids::strongest_bolid_light(
                &self.prepared_bolids,
                environment.render_view.camera_eye_world_m,
                self.renderer.atmosphere_gpu.optical_uniform(),
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
            Err(_) => {
                self.shadow_state.valid = false;
                self.shadow_state.caster_keys.clear();
                self.prepared_shadow = PreparedShadowFrame::invalid();
            }
        }
    }
}

fn directional_cascade(
    region: ShadowRegion,
    direction: [f64; 3],
    origin: [f64; 3],
    source_id: u32,
    lunar_radius: f64,
    sectors: &[ShadowSectorSnapshot],
    keys: &mut Vec<(SectorKey, u8)>,
) -> Option<ShadowUniform> {
    let mut direction = glam::DVec3::from_array(direction);
    let up = glam::DVec3::from_array(region.center_world).normalize_or_zero();
    let vertical = up - direction * up.dot(direction);
    let transition = ((0.35 - up.dot(direction)) / 0.10).clamp(0.0, 1.0);
    let limb_radius =
        if source_id == shadows::ShadowSource::Moon.id() && vertical.length_squared() > 1e-8 {
            lunar_radius * transition * transition * (3.0 - 2.0 * transition)
        } else {
            0.0
        };
    if limb_radius > 0.0 {
        direction = direction * limb_radius.cos() + vertical.normalize() * limb_radius.sin();
    }
    let mut projection = minimium_renderer::shadow_uniform::DirectionalShadowProjection::new(
        direction.to_array(),
        region.center_world,
        region.settings,
    )?;
    for sector in sectors {
        if sector.vertex_count != Some(0) && projection.include_caster(sector.bounds?) {
            keys.push((sector.key, sector.lod));
        }
    }
    let mut uniform = projection.uniform(origin, source_id)?;
    uniform.flags[3] = (limb_radius as f32).to_bits();
    Some(uniform)
}
