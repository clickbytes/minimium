use super::*;

impl Publication<'_> {
    pub(in crate::runtime::excavation) fn publish(
        &mut self,
        publication: coordinator::DurablePublication,
    ) -> io::Result<()> {
        let (prepared, origin) = publication.into_parts();
        let result = self.install(prepared, origin);
        if result.is_err() {
            self.request_shutdown();
        }
        result
    }

    fn install(&mut self, prepared: Prepared, origin: Origin) -> io::Result<()> {
        let started = Instant::now();
        let mutation = &prepared.mutation;
        if !self.impacts.checkpoints_match(&mutation.impact_bodies) {
            self.impact_failure("stale impact checkpoint before excavation publication");
            return Err(io::Error::other(
                "stale impact checkpoint before excavation publication",
            ));
        }
        let before = self.inputs(mutation.world).unwrap();
        for body in &mutation.impact_bodies {
            impacts::projection::prepare(self.worlds, self.components, *body, true)?;
        }
        if let Some(terrain) = prepared.terrain {
            let owner = self.worlds.get(mutation.world).unwrap();
            let manifest = owner.manifest;
            let collision_revision = owner
                .collision_source_revision
                .checked_add(1)
                .ok_or_else(|| io::Error::other("collision revision exhausted"))?;
            let sector_revision = minimium_world::world::SectorRevision::new(mutation.id).unwrap();
            self.sectors
                .publish_prepared(
                    mutation.world,
                    manifest.generation().revision(),
                    manifest.content_revision(),
                    sector_revision,
                    &prepared.sectors,
                )
                .map_err(|_| io::Error::other("sector ticket space exhausted"))?;
            for body in &mutation.impact_bodies {
                impacts::projection::publish(self.worlds, self.components, *body, true)?;
            }
            let mut after = before.clone();
            after.volumes = Some(terrain.clone());
            after.remnants = self
                .worlds
                .get(mutation.world)
                .map(|owner| owner.remnants.clone());
            if let Some(craters) = &prepared.craters {
                after.edits = Some(craters.clone());
            }
            self.sectors.invalidate_where(mutation.world, |key| {
                !prepared.sectors.contains_key(&key) && before.revision() != after.revision()
            });
            let tickets: HashSet<_> = self
                .sectors
                .installed()
                .filter(|(ticket, _, _)| ticket.instance() == mutation.world)
                .map(|(ticket, _, _)| ticket)
                .collect();
            let parts = tickets
                .iter()
                .filter_map(|ticket| {
                    self.sectors
                        .payload_arc(*ticket)
                        .map(|p| p.collision.clone())
                })
                .collect();
            let index = Arc::new(minimium_simulation::CollisionIndex::from_partitions(parts));
            let replacements = tickets
                .iter()
                .filter_map(|ticket| {
                    self.sectors
                        .payload_arc(*ticket)
                        .map(|payload| (ticket.key(), Arc::clone(&payload.collision)))
                })
                .collect();
            let owner = self.worlds.get_mut(mutation.world).unwrap();
            owner.volumes = Some(terrain);
            if let Some(craters) = prepared.craters {
                owner.terrain = craters;
            }
            owner
                .world
                .install_streamed_spatial_geometry(index, replacements, collision_revision);
            owner.collision_tickets = tickets;
            owner.collision_source_revision = collision_revision;
            owner.collision_installed_revision = collision_revision;
            self.state
                .metrics
                .collision
                .record(prepared.collision_elapsed);
        }
        if let Some((profile, inventory)) = mutation.inventory {
            self.state.inventories.insert(profile, inventory);
        }
        for body in &mutation.impact_bodies {
            if let Err(error) = self.impacts.acknowledge_checkpoint(*body) {
                self.impact_failure(&error.to_string());
                return Err(error);
            }
        }
        if let Some(water) = prepared.water {
            self.state.waters.insert(mutation.world, water);
        }
        if let Some(snapshot) = &mutation.fluids {
            if let Some(water) = self.state.waters.get_mut(&mutation.world) {
                water.mark_persisted(snapshot.revision());
                self.worlds
                    .get_mut(mutation.world)
                    .unwrap()
                    .world
                    .set_water_read(Some(water.water_read()));
            }
        }
        if let Some(action_id) = origin.action_id {
            self.action_result(
                origin,
                ActionResult::Committed {
                    action_id,
                    mutation_id: mutation.id,
                },
            );
        }
        self.state.metrics.publication.record(started.elapsed());
        Ok(())
    }
}
