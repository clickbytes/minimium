use super::*;
use minimium_world::terrain_edits::{length, project};

impl ServerRuntime {
    pub(in crate::runtime) fn spawn_meteorid(
        &mut self,
        manual: bool,
    ) -> crate::commands::CommandReply {
        let result = self.prepare_meteorid(manual);
        match result {
            Ok(id) => crate::commands::CommandReply::success(
                crate::commands::CommandPayload::ImpactSpawn { id },
            ),
            Err(error) => {
                crate::commands::CommandReply::error("server.impact.rejected", &error.to_string())
            }
        }
    }

    fn prepare_meteorid(&mut self, manual: bool) -> io::Result<u64> {
        let profile = self
            .impacts
            .profile
            .ok_or_else(|| io::Error::other("game has no impact profile"))?;
        if self.impacts.failure.is_some()
            || self.active_body_count() >= minimium_world::impact::MAX_ACTIVE_BODIES
            || self.impacts.body_count() + self.impacts.pending_count() >= 4096
        {
            return Err(io::Error::other(
                "impact service unavailable or capacity reached",
            ));
        }
        if manual
            && self
                .impacts
                .last_manual
                .is_some_and(|t| t.elapsed() < Duration::from_secs(5))
        {
            return Err(io::Error::other("meteorid rate limited; wait five seconds"));
        }
        let players = self.game.worlds.player_snapshots();
        let candidates: Vec<_> = players
            .iter()
            .filter(|(id, _)| {
                self.game
                    .worlds
                    .get(*id)
                    .is_some_and(|w| w.manifest.planet().is_some())
            })
            .collect();
        if candidates.is_empty() {
            return Err(io::Error::other("no active planet player"));
        }
        let &(world, ref player) =
            candidates[self.scheduler.simulation_tick as usize % candidates.len()];
        let owner = self.game.worlds.get(world).unwrap();
        let manifest = owner.manifest;
        let definition = manifest.planet().unwrap();
        let scale = if definition.mode == 1 {
            definition.height_scale_permille as f64 / 1000.0
        } else {
            1.0
        };
        let seed = mix(owner.entities.next_id()
            ^ manifest.generation().seed().get() as u64
            ^ self.scheduler.simulation_tick);
        let up = project(player.position.map(f64::from), 1.0);
        let axis = if up[1].abs() < 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let east = project(
            [
                up[1] * axis[2] - up[2] * axis[1],
                up[2] * axis[0] - up[0] * axis[2],
                up[0] * axis[1] - up[1] * axis[0],
            ],
            1.0,
        );
        let north = [
            up[1] * east[2] - up[2] * east[1],
            up[2] * east[0] - up[0] * east[2],
            up[0] * east[1] - up[1] * east[0],
        ];
        let geography = minimium_procgen::TerrainInputs {
            geography: self.prepared_geography.clone(),
            edits: Some(owner.terrain.clone()),
            remnants: Some(owner.remnants.clone()),
            volumes: owner.volumes.clone(),
        };
        let heading = self
            .sky_views
            .get(&player.id)
            .filter(|(context, _, _, at)| {
                self.game.worlds.context_matches(player.id, *context)
                    && at.elapsed() < Duration::from_secs(15)
            })
            .and_then(|(_, gaze, _, _)| {
                let x = (0..3).map(|i| f64::from(gaze[i]) * east[i]).sum::<f64>();
                let y = (0..3).map(|i| f64::from(gaze[i]) * north[i]).sum::<f64>();
                (x * x + y * y > 0.01).then(|| y.atan2(x))
            });
        let protected: Vec<[f64; 3]> = owner
            .entities
            .iter_components()
            .filter(|(_, kind, _)| *kind != BODY_COMPONENT)
            .filter_map(|(_, kind, blob)| {
                let minimium_world::entity::ReplicationVisibility::Spatial { position } =
                    self.component_registry.descriptor(kind)?.visibility
                else {
                    return None;
                };
                position(&blob.bytes).map(|p| p.map(f64::from))
            })
            .collect();
        let mut target = None;
        for attempt in 0..32 {
            let word = mix(seed + attempt);
            let jitter = (word % 10000) as f64 / 10000.0;
            let angle = if manual {
                heading.map_or(jitter * std::f64::consts::TAU, |h| h + (jitter - 0.5) * 1.2)
            } else {
                jitter * std::f64::consts::TAU
            };
            let distance = 220.0 + (word >> 16) as f64 % 650.0;
            let direction = std::array::from_fn(|i| {
                player.position[i] as f64
                    + (east[i] * angle.cos() + north[i] * angle.sin()) * distance
            });
            if let Some(point) = minimium_procgen::planet_point_at_direction_with_geography(
                manifest,
                direction.map(|v| v as f32),
                geography.clone(),
            )
            .filter(|p| !p.ocean)
            {
                let p = point.position_mm.coordinates().map(|v| v as f64 / 1000.0);
                let clearance = profile.crater_radius_m * 2.0 + 50.0;
                if protected
                    .iter()
                    .any(|q| length(std::array::from_fn(|i| p[i] - q[i])) < clearance)
                    || owner.map.boxes().iter().any(|b| {
                        length(std::array::from_fn(|i| p[i] - f64::from(b.center[i])))
                            < clearance + length(b.size.map(f64::from)) * 0.5
                    })
                    || !safe_footprint(
                        manifest,
                        p,
                        east,
                        north,
                        profile.crater_radius_m,
                        geography.clone(),
                    )
                {
                    continue;
                }
                if players
                    .iter()
                    .filter(|(w, _)| *w == world)
                    .all(|(_, player)| {
                        length(std::array::from_fn(|i| p[i] - player.position[i] as f64)) > 150.0
                    })
                {
                    target = Some(p);
                    break;
                }
            }
        }
        let target = target.ok_or_else(|| io::Error::other("no valid land impact near players"))?;
        let normal = project(target, 1.0);
        let start = std::array::from_fn(|i| {
            target[i] + normal[i] * 80000.0 * scale - east[i] * 16000.0 * scale
        });
        let velocity = project(std::array::from_fn(|i| target[i] - start[i]), 18000.0);
        let id = self.reserve_body_entity(world)?;
        let body = BodyState {
            world_seed: manifest.generation().seed().get(),
            id,
            world,
            seed,
            phase: 0,
            position: start,
            velocity,
            mass_kg: profile.initial_mass_kg,
            initial_mass_kg: profile.initial_mass_kg,
            density_kg_m3: profile.density_kg_m3,
            metres_scale: scale,
            planet_radius_m: definition.radius_mm as f64 / 1000.0,
            age_s: 0.0,
            crater_radius_m: profile.crater_radius_m,
            energy_j: 0.0,
            fragmentation: 0,
        };
        self.queue_body(body)?;
        if manual {
            self.impacts.last_manual = Some(Instant::now());
        }
        Ok(id)
    }
}

fn safe_footprint(
    manifest: minimium_world::world::WorldManifest,
    center: [f64; 3],
    east: [f64; 3],
    north: [f64; 3],
    radius: f64,
    geography: minimium_procgen::TerrainInputs,
) -> bool {
    let base = length(center);
    (0..8).all(|i| {
        let angle = f64::from(i) * std::f64::consts::TAU / 8.0;
        let direction = std::array::from_fn(|a| {
            center[a] + (east[a] * angle.cos() + north[a] * angle.sin()) * radius * 2.0
        });
        minimium_procgen::planet_point_at_direction_with_geography(
            manifest,
            direction.map(|x| x as f32),
            geography.clone(),
        )
        .is_some_and(|p| !p.ocean && (p.height_mm as f64 / 1000.0 - base).abs() < radius * 0.75)
    })
}

pub(super) fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
