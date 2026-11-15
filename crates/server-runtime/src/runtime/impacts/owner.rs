use super::*;

pub(crate) struct ProtectedImpactEntities(std::collections::BTreeSet<(WorldInstanceId, u64)>);

impl ProtectedImpactEntities {
    pub(crate) fn contains(&self, world: WorldInstanceId, entity: u64) -> bool {
        self.0.contains(&(world, entity))
    }

    #[cfg(test)]
    pub(crate) fn test_empty() -> Self {
        Self(std::collections::BTreeSet::new())
    }
}

impl Impacts {
    pub(in crate::runtime) fn bodies(&self) -> impl Iterator<Item = &BodyState> {
        self.bodies.values()
    }

    pub(in crate::runtime) fn body(&self, key: &(WorldInstanceId, u64)) -> Option<&BodyState> {
        self.bodies.get(key)
    }

    pub(in crate::runtime) fn body_count(&self) -> usize {
        self.bodies.len()
    }

    pub(in crate::runtime) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub(in crate::runtime) fn contains_body(&self, key: &(WorldInstanceId, u64)) -> bool {
        self.bodies.contains_key(key)
    }

    pub(in crate::runtime) fn contains_pending(&self, key: &(WorldInstanceId, u64)) -> bool {
        self.pending.contains_key(key)
    }

    pub(in crate::runtime) fn has_pending_in_world(&self, world: WorldInstanceId) -> bool {
        self.pending
            .keys()
            .any(|(pending_world, _)| *pending_world == world)
    }

    pub(in crate::runtime) fn protected_entities(&self) -> ProtectedImpactEntities {
        ProtectedImpactEntities(
            self.bodies
                .keys()
                .chain(self.pending.keys())
                .copied()
                .collect(),
        )
    }

    pub(super) fn advance_live_body(&mut self, body: BodyState) -> Result<(), &'static str> {
        let current = self
            .bodies
            .get_mut(&(body.world, body.id))
            .ok_or("impact body is not live")?;
        if !body.valid() || !body.follows(current) {
            return Err("impact body update violates durable identity or progression");
        }
        *current = body;
        Ok(())
    }

    pub(in crate::runtime) fn restore_live_body(&mut self, body: BodyState) -> std::io::Result<()> {
        if !body.valid() {
            return Err(std::io::Error::other("invalid restored impact body"));
        }
        let key = (body.world, body.id);
        match self.bodies.get(&key) {
            Some(current) if body.follows(current) => {
                self.bodies.insert(key, body);
            }
            Some(current) if current.follows(&body) => {}
            Some(_) => {
                return Err(std::io::Error::other(
                    "restored impact body conflicts with live identity or progression",
                ));
            }
            None => {
                self.bodies.insert(key, body);
            }
        }
        Ok(())
    }

    pub(in crate::runtime) fn checkpoint_pending_matches(&self, body: &BodyState) -> bool {
        self.pending.get(&(body.world, body.id)) == Some(body)
    }

    pub(in crate::runtime) fn can_acknowledge_checkpoint(&self, body: &BodyState) -> bool {
        self.checkpoint_pending_matches(body)
            && self
                .bodies
                .get(&(body.world, body.id))
                .is_none_or(|current| current.follows(body) || body.follows(current))
    }

    pub(in crate::runtime) fn checkpoints_match(&self, bodies: &[BodyState]) -> bool {
        let mut identities = std::collections::BTreeSet::new();
        bodies.iter().all(|body| {
            identities.insert((body.world, body.id)) && self.can_acknowledge_checkpoint(body)
        })
    }

    pub(in crate::runtime) fn acknowledge_checkpoint(
        &mut self,
        body: BodyState,
    ) -> std::io::Result<()> {
        if !self.can_acknowledge_checkpoint(&body) {
            return Err(std::io::Error::other(
                "stale impact checkpoint acknowledgement",
            ));
        }
        let key = (body.world, body.id);
        match self.bodies.get(&key) {
            Some(current) if current.follows(&body) => {}
            Some(current) if body.follows(current) => {
                self.bodies.insert(key, body);
            }
            Some(_) => {
                return Err(std::io::Error::other(
                    "impact checkpoint acknowledgement conflicts with live progression",
                ));
            }
            None => {
                self.bodies.insert(key, body);
            }
        }
        self.pending.remove(&key);
        Ok(())
    }

    pub(in crate::runtime) fn discard_pending_checkpoint(&mut self, body: &BodyState) -> bool {
        let key = (body.world, body.id);
        if self.pending.get(&key) != Some(body) {
            return false;
        }
        self.pending.remove(&key);
        true
    }

    pub(in crate::runtime) fn begin_checkpoints(
        &mut self,
        bodies: &[BodyState],
    ) -> std::io::Result<()> {
        let mut identities = std::collections::BTreeSet::new();
        for body in bodies {
            let key = (body.world, body.id);
            if !body.valid()
                || !identities.insert(key)
                || self.pending.contains_key(&key)
                || self
                    .bodies
                    .get(&key)
                    .is_some_and(|current| !body.follows(current))
            {
                return Err(std::io::Error::other(
                    "impact checkpoint conflicts with pending identity or progression",
                ));
            }
        }
        for body in bodies {
            self.pending.insert((body.world, body.id), *body);
        }
        Ok(())
    }
}

impl Impacts {
    pub(in crate::runtime) fn fail(
        &mut self,
        logger: &minimium_diagnostics::DomainLogger,
        error: io::Error,
    ) {
        crate::diagnostics::emit(
            logger,
            minimium_diagnostics::Level::Error,
            "world.impact_failed",
            &[("reason", error.to_string())],
        );
        self.failure = Some(error.to_string());
    }
}
