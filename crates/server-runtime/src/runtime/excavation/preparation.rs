use super::*;

pub(super) struct ToolPreparation {
    pub request: TerrainEditRequest,
    pub manifest: minimium_world::world::WorldManifest,
    pub map: Arc<WorldMap>,
    pub inputs: minimium_procgen::TerrainInputs,
    pub store: Arc<TerrainStore>,
    pub water: Option<water::WaterWorld>,
    pub water_budget: usize,
    pub water_catalog_digest: Option<[u8; 32]>,
}

impl ToolPreparation {
    pub(super) fn run(mut self) -> Result<Prepared, PreparationError> {
        let terrain = scene::planet(self.manifest, self.inputs.clone())
            .ok_or(ActionRejection::InvalidIntent)?;
        let base = terrain
            .volume_base()
            .ok_or(ActionRejection::InvalidIntent)?;
        let edit = self
            .store
            .prepare(&base, self.request)
            .map_err(scene::edit_error)?;
        let mut mutation = WorldMutation {
            id: self.request.mutation_id,
            world: self.store.world(),
            world_seed: self.manifest.generation().seed().get(),
            expected_chunk_revisions: edit.expected().to_vec(),
            resulting_chunks: edit.chunks().to_vec(),
            inventory: None,
            fluids: None,
            impact_bodies: Vec::new(),
            craters: Vec::new(),
            water_catalog_digest: self.water_catalog_digest,
        };
        let mut candidate = (*self.store).clone();
        candidate.commit(edit).map_err(scene::edit_error)?;
        let candidate = Arc::new(candidate);
        let overlays = minimium_procgen::TerrainOverlaySnapshot::new(
            self.inputs.source().world(),
            self.inputs.overlays().edits().cloned(),
            self.inputs.overlays().remnants().cloned(),
            Some(candidate.clone()),
        );
        self.inputs = minimium_procgen::TerrainInputs::new(self.inputs.source().clone(), overlays)
            .map_err(|_| ActionRejection::InvalidIntent)?;
        let candidate_terrain =
            scene::planet(self.manifest, self.inputs).ok_or(ActionRejection::InvalidIntent)?;
        if let Some(state) = &mut self.water {
            state
                .reserve_cell_limit(
                    self.water_budget
                        .min(state.snapshot().config().max_active_cells),
                )
                .map_err(|_| ActionRejection::Capacity)?;
            let candidate_base = candidate_terrain
                .volume_base()
                .ok_or(ActionRejection::InvalidIntent)?;
            let prepared_water = state
                .prepare_terrain_edit(
                    &self.store.view(&base),
                    &candidate.view(&candidate_base),
                    self.request,
                    self.water_budget,
                )
                .map_err(|_| ActionRejection::Capacity)?;
            mutation.fluids = Some(prepared_water.snapshot().clone());
            state.publish_terrain_edit(prepared_water);
        }
        let changed: Vec<_> = mutation
            .resulting_chunks
            .iter()
            .map(|chunk| chunk.key())
            .collect();
        let chunk_coordinates = changed.iter().take(8).map(|key| key.coordinates).collect();
        let collision_started = Instant::now();
        let geometries = minimium_procgen::validate_volume_geometry(
            &self.map,
            self.manifest,
            &candidate_terrain,
            &changed,
        )
        .map_err(|reason| {
            PreparationError::Geometry(GeometryFailure {
                reason,
                mutation_id: self.request.mutation_id,
                center_m: self.request.center_m,
                chunk_coordinates,
            })
        })?;
        let sectors = geometries
            .into_iter()
            .map(|(key, geometry)| (key, Arc::new(SectorPayload::new(geometry))))
            .collect();
        Ok(Prepared {
            mutation,
            terrain: Some(candidate),
            sectors,
            collision_elapsed: collision_started.elapsed(),
            water: self.water,
            craters: None,
        })
    }
}
