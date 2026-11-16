//! Private owner for confirmed profile equipment and temporary actor requests.
use minimium_core::{identity::PlayerProfileId, ids::PlayerId};
use minimium_gameplay::WorldChangePlan;
use minimium_protocol::{ExcavationInput, WorldContext};
use minimium_simulation::tools::{ToolAction, ToolProfile, ToolScene, ToolUseGate};
use minimium_world::items::{ActionRejection, ActionResult, Inventory, ToolUseIntent};
use std::collections::HashMap;

pub(super) struct Equipment {
    confirmed: HashMap<PlayerProfileId, Inventory>,
    requests: HashMap<PlayerId, (WorldContext, Inventory)>,
    reservation: Option<InventoryReservation>,
    gates: HashMap<PlayerId, (WorldContext, ToolUseGate)>,
}

struct InventoryReservation {
    mutation_id: u64,
    profile: PlayerProfileId,
    expected_revision: u64,
    candidate: Inventory,
}

pub(super) struct InventoryCommit {
    profile: PlayerProfileId,
    candidate: Inventory,
}

impl Equipment {
    pub(super) fn from_restored(
        confirmed: HashMap<PlayerProfileId, Inventory>,
        max_profiles: usize,
    ) -> Self {
        Self {
            confirmed: confirmed.into_iter().collect(),
            requests: HashMap::with_capacity(max_profiles),
            reservation: None,
            gates: HashMap::with_capacity(max_profiles),
        }
    }

    pub(super) fn confirmed(&self, profile: PlayerProfileId) -> Inventory {
        self.confirmed.get(&profile).copied().unwrap_or_default()
    }

    pub(super) fn confirmed_for(&self, profile: Option<PlayerProfileId>) -> Inventory {
        profile.map_or_else(Inventory::default, |profile| self.confirmed(profile))
    }

    /// Overlay queued intent first, then the accepted journal candidate, then confirmed state.
    pub(super) fn desired(
        &self,
        player: PlayerId,
        context: WorldContext,
        profile: PlayerProfileId,
    ) -> Inventory {
        self.requests
            .get(&player)
            .filter(|(epoch, _)| *epoch == context)
            .map(|(_, inventory)| *inventory)
            .or_else(|| self.pending(profile))
            .unwrap_or_else(|| self.confirmed(profile))
    }

    pub(super) fn pending(&self, profile: PlayerProfileId) -> Option<Inventory> {
        self.reservation
            .as_ref()
            .filter(|reservation| reservation.profile == profile)
            .map(|reservation| reservation.candidate)
    }

    pub(super) fn request(&self, player: PlayerId) -> Option<(WorldContext, Inventory)> {
        self.requests.get(&player).copied()
    }

    pub(super) fn set_request(
        &mut self,
        player: PlayerId,
        context: WorldContext,
        inventory: Inventory,
    ) {
        self.requests.insert(player, (context, inventory));
    }

    pub(super) fn request_change(
        &mut self,
        player: PlayerId,
        context: WorldContext,
        profile: PlayerProfileId,
        input: ExcavationInput,
        grants: [Option<minimium_world::items::ItemRecord>; 2],
    ) -> Result<Option<Inventory>, ActionRejection> {
        let submitted = self.pending(profile);
        let old = self.desired(player, context, profile);
        let mut candidate = old;
        match input {
            ExcavationInput::GrantTools => candidate.ensure_items(grants).map(|_| ())?,
            ExcavationInput::Equip { slot } => candidate.equip(slot)?,
            _ => return Err(ActionRejection::InvalidIntent),
        }
        if old != candidate {
            self.set_request(player, context, candidate);
            return Ok(None);
        }
        if submitted.is_none() && self.request(player).is_none() {
            return Ok(Some(candidate));
        }
        Ok(None)
    }

    pub(super) fn remove_request(&mut self, player: PlayerId) {
        self.requests.remove(&player);
    }

    pub(super) fn retain_requests(
        &mut self,
        mut current: impl FnMut(PlayerId, WorldContext) -> bool,
    ) {
        self.requests
            .retain(|player, (context, _)| current(*player, *context));
    }

    pub(super) fn next_request(&self) -> Option<(PlayerId, WorldContext, Inventory)> {
        self.requests
            .iter()
            .next()
            .map(|(player, (context, inventory))| (*player, *context, *inventory))
    }

    pub(super) fn prepare_tool_use(
        &mut self,
        player: PlayerId,
        context: WorldContext,
        profile: ToolProfile,
        player_profile: Option<PlayerProfileId>,
        intent: ToolUseIntent,
        scene: &dyn ToolScene,
        now_ms: u64,
    ) -> Result<ToolAction, ActionRejection> {
        let inventory = self.confirmed_for(player_profile);
        let gate = self.gates.entry(player).or_insert_with(|| {
            (
                context,
                ToolUseGate::new(profile).expect("validated runtime tool profile"),
            )
        });
        if gate.0 != context {
            *gate = (
                context,
                ToolUseGate::new(profile).expect("validated runtime tool profile"),
            );
        }
        gate.1.prepare(intent, inventory, scene, now_ms)
    }

    pub(super) fn completed_action(
        &self,
        player: PlayerId,
        context: WorldContext,
        action_id: u64,
    ) -> Option<ActionResult> {
        self.gates
            .get(&player)
            .filter(|(epoch, _)| *epoch == context)
            .and_then(|(_, gate)| gate.completed(action_id))
    }

    pub(super) fn finish_action(
        &mut self,
        player: PlayerId,
        context: WorldContext,
        result: ActionResult,
    ) {
        if let Some((epoch, gate)) = self.gates.get_mut(&player) {
            if *epoch == context {
                gate.finish(result);
            }
        }
    }

    pub(super) fn retain_actors(&mut self, mut active: impl FnMut(PlayerId) -> bool) {
        self.gates.retain(|player, _| active(*player));
        self.requests.retain(|player, _| active(*player));
    }

    pub(super) fn remove_actor(&mut self, player: PlayerId) {
        self.gates.remove(&player);
        self.requests.remove(&player);
    }

    /// Reserve only the compact candidate. Coordinator ACK matching proves this ID
    /// belongs to the exact opaque plan before it can be published.
    pub(super) fn reserve_plan(&mut self, plan: &WorldChangePlan) -> Result<(), ActionRejection> {
        let Some((profile, candidate)) = plan.inventory() else {
            return Ok(());
        };
        if self.reservation.is_some() {
            return Err(ActionRejection::Busy);
        }
        let expected_revision = self.confirmed(profile).revision;
        let Some(next_revision) = expected_revision.checked_add(1) else {
            return Err(ActionRejection::Capacity);
        };
        if candidate.revision != next_revision {
            return Err(ActionRejection::Stale);
        }
        self.reservation = Some(InventoryReservation {
            mutation_id: plan.id(),
            profile,
            expected_revision,
            candidate,
        });
        Ok(())
    }

    /// Called only after the coordinator matches the ACK to its exact submitted plan.
    pub(super) fn preflight_commit(
        &mut self,
        plan: &WorldChangePlan,
    ) -> Result<Option<InventoryCommit>, ()> {
        let Some((profile, candidate)) = plan.inventory() else {
            return Ok(None);
        };
        let Some(reservation) = self.reservation.as_ref() else {
            return Err(());
        };
        if reservation.mutation_id != plan.id()
            || reservation.profile != profile
            || reservation.expected_revision != self.confirmed(profile).revision
            || reservation.candidate != candidate
        {
            return Err(());
        }
        let reservation = self.reservation.take().ok_or(())?;
        Ok(Some(InventoryCommit {
            profile: reservation.profile,
            candidate: reservation.candidate,
        }))
    }

    pub(super) fn commit(&mut self, commit: InventoryCommit) {
        self.confirmed.insert(commit.profile, commit.candidate);
    }

    pub(super) fn cancel_plan(&mut self, mutation_id: u64) {
        if self
            .reservation
            .as_ref()
            .is_some_and(|reservation| reservation.mutation_id == mutation_id)
        {
            self.reservation = None;
        }
    }
}

impl crate::runtime::ServerRuntime {
    pub(super) fn player_profile(&self, player: PlayerId) -> Option<PlayerProfileId> {
        self.connections.clients.iter().find_map(|client| {
            client
                .identity()
                .filter(|identity| identity.player == player)
                .map(|identity| identity.principal.profile())
        })
    }
}
