//! Immutable object snapshots, with a lifecycle independent of ground deformation.
use crate::{
    impact::BodyState,
    terrain_edits::{length, project},
};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct RemnantSnapshot {
    radius_m: f64,
    bodies: BTreeMap<u64, BodyState>,
    revision: u64,
}

impl RemnantSnapshot {
    pub fn empty(radius_m: f64) -> Self {
        Self {
            radius_m,
            bodies: BTreeMap::new(),
            revision: 0,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn set(&mut self, body: BodyState) -> Result<bool, &'static str> {
        if !body.valid()
            || !self.radius_m.is_finite()
            || (self.radius_m - body.planet_radius_m).abs() > 0.001
        {
            return Err("invalid remnant or planet radius");
        }
        let changed = if body.phase == 1 {
            self.bodies.get(&body.id) != Some(&body)
        } else {
            self.bodies.contains_key(&body.id)
        };
        if !changed {
            return Ok(false);
        }
        if body.phase == 1 && self.bodies.len() >= 4096 && !self.bodies.contains_key(&body.id) {
            return Err("remnant capacity exceeded");
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("remnant revision exhausted")?;
        if body.phase == 1 {
            self.bodies.insert(body.id, body);
        } else {
            self.bodies.remove(&body.id);
        }
        self.revision = revision;
        Ok(true)
    }

    pub fn nearby(&self, direction: [f64; 3], bound: f64) -> impl Iterator<Item = &BodyState> {
        let center = project(direction, self.radius_m);
        self.bodies.values().filter(move |body| {
            let p = project(body.position, self.radius_m);
            length(std::array::from_fn(|i| p[i] - center[i])) <= bound + body.crater_radius_m * 2.0
        })
    }
}
