use super::*;
use minimium_world::{impact::BodyState, terrain::TerrainChunkKey};

impl ServerRuntime {
    pub(in crate::runtime) fn restore_excavation_impacts(&mut self) -> io::Result<()> {
        for (world, saved) in &self.excavation.state.restored.worlds {
            let owner = self
                .game
                .worlds
                .get_mut(*world)
                .ok_or_else(|| io::Error::other("unknown impact checkpoint world"))?;
            if owner.manifest.generation().seed().get() != saved.world_seed {
                return Err(io::Error::other(
                    "impact checkpoint world identity mismatch",
                ));
            }
            for stamp in saved.craters.values() {
                Arc::make_mut(&mut owner.terrain)
                    .insert(*stamp)
                    .map_err(io::Error::other)?;
            }
            for body in saved.impact_bodies.values() {
                self.impacts.restore_live_body(*body)?;
            }
        }
        Ok(())
    }

    pub(in crate::runtime) fn queue_impact_excavation(
        &mut self,
        body: BodyState,
    ) -> io::Result<()> {
        if self.excavation.busy()
            || self.excavation.state.water_jobs.contains_key(&body.world)
            || self.impacts.has_pending_in_world(body.world)
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "impact terrain is reserved",
            ));
        }
        if let Some(error) = self.excavation.coordinator.failure() {
            return Err(io::Error::other(error.to_owned()));
        }
        if self
            .excavation
            .state
            .water_failures
            .contains_key(&body.world)
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "impact waits for the world's fluid system",
            ));
        }
        let stamp = body
            .crater()
            .ok_or_else(|| io::Error::other("impact has no crater result"))?;
        self.reserve_body_budget(body)?;
        let inputs = self
            .excavation_inputs(body.world)
            .ok_or_else(|| io::Error::other("impact world missing"))?;
        let input_revision = inputs.revision();
        let owner = self.game.worlds.get(body.world).unwrap();
        let manifest = owner.manifest;
        let map = owner.map.clone();
        let store = owner
            .volumes
            .clone()
            .ok_or_else(|| io::Error::other("impact terrain unavailable"))?;
        let mut craters = owner.terrain.clone();
        Arc::make_mut(&mut craters)
            .insert(stamp)
            .map_err(io::Error::other)?;
        let mut remnants = owner.remnants.clone();
        Arc::make_mut(&mut remnants)
            .set(body)
            .map_err(io::Error::other)?;
        let id = self
            .excavation
            .allocate_id()
            .ok_or_else(|| io::Error::other("mutation IDs exhausted"))?;
        let mut water = self.excavation.state.waters.get(&body.world).cloned();
        let digest = self
            .excavation
            .state
            .water_catalogs
            .get(&body.world)
            .copied();
        let other_cells: usize = self
            .excavation
            .state
            .waters
            .iter()
            .filter(|(world, _)| **world != body.world)
            .map(|(_, w)| w.reserved_cell_limit())
            .sum();
        let budget = self
            .excavation
            .state
            .config
            .server_fluid_cells
            .saturating_sub(other_cells)
            .min(self.excavation.state.config.fluids.max_active_cells);
        self.excavation.coordinator.prepare(
            PreparationRequest {
                origin: Origin::default(),
                world: body.world,
                impact_bodies: vec![body],
                input_revision,
            },
            Box::new(move || {
                let old_planet =
                    scene::planet(manifest, inputs.clone()).ok_or(ActionRejection::Stale)?;
                let old_base = old_planet
                    .volume_base()
                    .ok_or(ActionRejection::InvalidIntent)?;
                let mut candidate_inputs = inputs;
                candidate_inputs.edits = Some(craters.clone());
                candidate_inputs.remnants = Some(remnants);
                let new_planet = scene::planet(manifest, candidate_inputs.clone())
                    .ok_or(ActionRejection::Stale)?;
                let new_base = new_planet
                    .volume_base()
                    .ok_or(ActionRejection::InvalidIntent)?;
                let mut mutation = WorldMutation {
                    id,
                    world: body.world,
                    world_seed: body.world_seed,
                    expected_chunk_revisions: Vec::new(),
                    resulting_chunks: Vec::new(),
                    inventory: None,
                    fluids: None,
                    impact_bodies: vec![body],
                    craters: vec![stamp],
                    water_catalog_digest: digest,
                };
                let candidate = match store.prepare_crater(&old_base, &new_base, stamp, id) {
                    Ok(edit) => {
                        mutation.expected_chunk_revisions = edit.expected().to_vec();
                        mutation.resulting_chunks = edit.chunks().to_vec();
                        let mut candidate = (*store).clone();
                        candidate.commit(edit).map_err(scene::edit_error)?;
                        candidate
                    }
                    Err(minimium_terrain::TerrainEditError::NoMaterial) => {
                        TerrainStore::from_chunks(
                            store.world(),
                            store.config(),
                            id,
                            store.chunks().cloned(),
                        )
                        .ok_or(ActionRejection::Capacity)?
                    }
                    Err(error) => return Err(scene::edit_error(error).into()),
                };
                let candidate = Arc::new(candidate);
                if let Some(water) = &mut water {
                    water
                        .reserve_cell_limit(budget.min(water.snapshot().config().max_active_cells))
                        .map_err(|_| ActionRejection::Capacity)?;
                    let prepared = water
                        .prepare_crater_water(
                            &store.view(&old_base),
                            &candidate.view(&new_base),
                            stamp,
                            budget,
                        )
                        .map_err(|_| ActionRejection::Capacity)?;
                    mutation.fluids = Some(prepared.snapshot().clone());
                    water.publish_terrain_edit(prepared);
                    if water.snapshot().cells().is_empty() {
                        water
                            .reserve_cell_limit(0)
                            .map_err(|_| ActionRejection::Capacity)?;
                    }
                }
                candidate_inputs.volumes = Some(candidate.clone());
                let candidate_planet =
                    scene::planet(manifest, candidate_inputs).ok_or(ActionRejection::Stale)?;
                let mut affected: Vec<_> =
                    mutation.resulting_chunks.iter().map(|c| c.key()).collect();
                for corner in 0..8 {
                    let position = std::array::from_fn(|i| {
                        stamp.center[i]
                            + if corner & (1 << i) == 0 { -2.0 } else { 2.0 } * stamp.radius_m
                    });
                    affected.push(
                        TerrainChunkKey::at(body.world, position)
                            .ok_or(ActionRejection::InvalidIntent)?,
                    );
                }
                let started = Instant::now();
                let chunk_coordinates =
                    affected.iter().take(8).map(|key| key.coordinates).collect();
                let sectors = minimium_procgen::validate_volume_geometry(
                    &map,
                    manifest,
                    &candidate_planet,
                    &affected,
                )
                .map_err(|reason| {
                    PreparationError::Geometry(GeometryFailure {
                        reason,
                        mutation_id: id,
                        center_m: stamp.center,
                        chunk_coordinates,
                    })
                })?
                .into_iter()
                .map(|(key, geometry)| (key, Arc::new(SectorPayload::new(geometry))))
                .collect();
                Ok(Prepared {
                    mutation,
                    terrain: Some(candidate),
                    sectors,
                    collision_elapsed: started.elapsed(),
                    water,
                    craters: Some(craters),
                })
            }),
            &mut ports::WorkSubmission::new(&self.sector_workers, &mut self.impacts),
        )
    }
}
