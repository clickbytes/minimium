use super::*;

pub(super) fn query(
    model: &mut WeatherModel,
    observer: DVec3,
    utc: f64,
    revision: u64,
) -> CloudSnapshot {
    // A cap expanded by lifetime * fastest wind includes every incoming source,
    // including paths across cube faces. Region size is never a weather boundary.
    let sources = region::intersecting(
        observer,
        OUTER_ANGLE + WIND[2] * LIFETIME / EARTH_RADIUS + 0.006,
    );
    let epoch = (utc / POPULATION_SECONDS).floor() as i64;
    model.cache.retain(|_, cloud| {
        utc >= cloud.birth_utc - POPULATION_SECONDS && utc <= cloud.birth_utc + LIFETIME
    });
    let mut clouds = std::collections::BTreeMap::new();
    let mut limited = false;
    for source in &sources {
        for layer in 0..3 {
            for population in epoch - 2..=epoch {
                for slot in 0..4 {
                    let id = hash(
                        model.seed()
                            ^ region::address(*source).wrapping_mul(0x9e3779b97f4a7c15)
                            ^ (layer as u64 * 17 + slot).wrapping_mul(0xd6e8feb86659fd93)
                            ^ hash(population as u64),
                    );
                    let birth = population as f64 * POPULATION_SECONDS
                        + random(id ^ 1) * POPULATION_SECONDS;
                    if utc < birth || utc > birth + LIFETIME {
                        continue;
                    }
                    let cloud = if let Some(cloud) = model.cache.get(&id) {
                        *cloud
                    } else {
                        let direction = region::point(*source, random(id ^ 2), random(id ^ 3));
                        let conditions = model.sample(direction, birth);
                        let mut cloud = Cloud {
                            id,
                            source: *source,
                            layer,
                            birth_utc: birth,
                            direction,
                            base_m: conditions.layers[layer].base_m,
                            kind: CloudKind::Cumulus,
                            size_m: DVec3::ONE,
                            min_level: 2,
                            max_level: 6,
                            opacity: 0.0,
                        };
                        let (max_base, ceiling) =
                            families::configure(model, conditions, &mut cloud);
                        // Coverage controls occurrence as well as optical thickness.
                        if random(id ^ 0x18b7) < conditions.layers[layer].coverage * 0.65 {
                            cloud.base_m = cloud.base_m.max(trajectory_floor(model, cloud));
                            if cloud.base_m > max_base || cloud.base_m + cloud.size_m.y > ceiling {
                                cloud.opacity = 0.0;
                            }
                        } else {
                            cloud.opacity = 0.0;
                        }
                        if model.cache.len() < MAX_CACHED_CLOUDS {
                            model.cache.insert(id, cloud);
                        }
                        cloud
                    };
                    if cloud.opacity > 0.005
                        && cloud.direction_at(utc).dot(observer)
                            >= (OUTER_ANGLE + cloud.extent_m() / model.radius).cos()
                    {
                        if clouds.len() < MAX_CLOUDS {
                            clouds.insert(cloud.id, cloud);
                        } else {
                            limited = true;
                            if clouds
                                .last_key_value()
                                .is_some_and(|(id, _)| cloud.id < *id)
                            {
                                clouds.pop_last();
                                clouds.insert(cloud.id, cloud);
                            }
                        }
                    }
                }
            }
        }
    }
    // A fixed ID order makes budget degradation reproducible and independent of view.
    let clouds = clouds.into_values().collect();
    CloudSnapshot {
        utc,
        revision,
        observer,
        conditions: model.sample(observer, utc),
        region: region::at(observer),
        regions: region::intersecting(observer, OUTER_ANGLE),
        clouds,
        limited,
    }
}

fn trajectory_floor(model: &WeatherModel, cloud: Cloud) -> f64 {
    // Bound the entire advected footprint, not only sampled terrain points.
    let midpoint = cloud.direction_at(cloud.birth_utc + LIFETIME * 0.5);
    let angle = cloud.extent_m() / model.radius + WIND[cloud.layer] * LIFETIME * 0.5 / EARTH_RADIUS;
    let floor = if let Some(p) = model.manifest.planet() {
        let limit = f64::from(p.mountain_limit_mm) / 1000.0;
        if p.mode == 1 {
            minimium_procgen::earth_elevation_upper_bound_m(midpoint.to_array(), angle)
                .map(|height| {
                    (height * f64::from(p.height_scale_permille) / 1000.0
                        + f64::from(p.detail_strength_mm) / 1000.0)
                        .max(p.sea_level_mm as f64 / 1000.0)
                        .min(limit)
                })
                .unwrap_or(limit)
        } else {
            limit
        }
    } else {
        minimium_procgen::planet_surface_for(model.manifest).relief_mm() as f64 / 1000.0
    };
    floor.max(0.0) + 500.0 * model.scale
}
