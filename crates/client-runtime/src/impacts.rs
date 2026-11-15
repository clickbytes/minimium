//! Context-bound meteor objects. Terrain has an independent excavation replica.
use minimium_protocol::WorldContext;
use minimium_world::{impact::BodyState, remnants::RemnantSnapshot};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct RemnantCache {
    body_revision: u64,
    crater_snapshot: usize,
    value: Option<Arc<RemnantSnapshot>>,
}

#[derive(Default)]
pub struct ImpactReplica {
    pub(crate) context: Option<WorldContext>,
    pub(crate) ready: bool,
    pub(crate) bodies: BTreeMap<u64, (BodyState, u64)>,
    remnant_revision: u64,
    remnant_cache: Mutex<RemnantCache>,
}

impl ImpactReplica {
    pub(crate) fn begin(&mut self, context: WorldContext) {
        *self = Self {
            context: Some(context),
            ..Self::default()
        };
    }

    pub(crate) fn apply(
        &mut self,
        context: WorldContext,
        body: BodyState,
        time: u64,
    ) -> Result<(), &'static str> {
        if self.context != Some(context) || body.world != context.instance() {
            return Err("impact context mismatch");
        }
        if self.bodies.len() >= 4096 && !self.bodies.contains_key(&body.id) {
            return Err("impact replica capacity exceeded");
        }
        let previous = self.bodies.get(&body.id).copied();
        if let Some((old, old_time)) = previous {
            if !body.follows(&old) || old_time > time {
                return Err("impact state regressed");
            }
            if old.phase != 0
                && (old.position != body.position || old.crater_radius_m != body.crater_radius_m)
            {
                return Err("committed crater changed");
            }
        }
        let remnant_changed = (body.phase == 1 || previous.is_some_and(|(old, _)| old.phase == 1))
            && previous.is_none_or(|(old, _)| {
                old.phase != body.phase
                    || old.position != body.position
                    || old.crater_radius_m != body.crater_radius_m
            });
        let next_remnant_revision = if remnant_changed {
            Some(
                self.remnant_revision
                    .checked_add(1)
                    .ok_or("remnant cache revision exhausted")?,
            )
        } else {
            None
        };
        self.bodies.insert(body.id, (body, time));
        if let Some(revision) = next_remnant_revision {
            self.remnant_revision = revision;
        }
        Ok(())
    }

    pub(crate) fn remnants_for_craters(
        &self,
        craters: Option<&Arc<minimium_terrain::TerrainSnapshot>>,
    ) -> Option<Arc<RemnantSnapshot>> {
        let crater_snapshot = craters.map_or(0, |snapshot| Arc::as_ptr(snapshot) as usize);
        let mut cache = self.remnant_cache.lock().ok()?;
        if cache.body_revision == self.remnant_revision && cache.crater_snapshot == crater_snapshot
        {
            return cache.value.as_ref().map(Arc::clone);
        }
        let crater_ids: BTreeSet<_> = craters
            .into_iter()
            .flat_map(|snapshot| snapshot.edits().iter().map(|stamp| stamp.id))
            .collect();
        let radius = craters.map_or_else(
            || {
                self.bodies
                    .values()
                    .next()
                    .map_or(1.0, |(body, _)| body.planet_radius_m)
            },
            |snapshot| snapshot.radius_m(),
        );
        let mut remnants = RemnantSnapshot::empty(radius);
        for (body, _) in self.bodies.values() {
            if body.phase == 1 && crater_ids.contains(&body.id) {
                remnants.set(*body).ok()?;
            }
        }
        let value = Arc::new(remnants);
        cache.body_revision = self.remnant_revision;
        cache.crater_snapshot = crater_snapshot;
        cache.value = Some(Arc::clone(&value));
        Some(value)
    }
}
