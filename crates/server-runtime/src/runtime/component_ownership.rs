use minimium_world::{
    entity::{ComponentOwnerId, ComponentTypeId, EntityId},
    world::WorldInstanceId,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Weak,
};

struct Issuer;
struct WorldLease;
struct PortLease {
    active: AtomicBool,
}

/// Host-held authority scoped to one registered component owner and world.
/// Clones share revocation state; construction is only available to ServerRuntime.
#[derive(Clone)]
pub struct ComponentOwnerPort {
    owner: ComponentOwnerId,
    world: WorldInstanceId,
    id: u64,
    issuer: Weak<Issuer>,
    world_lease: Weak<WorldLease>,
    lease: Weak<PortLease>,
}

impl ComponentOwnerPort {
    pub fn world(&self) -> WorldInstanceId {
        self.world
    }

    pub(crate) fn owner(&self) -> &ComponentOwnerId {
        &self.owner
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentOwnerError {
    UnknownComponent,
    GeneralComponent,
    UnknownWorld,
    ForeignPort,
    Revoked,
    ReservationLimitExceeded,
    InvalidMutation,
    StorageBudget,
    Validation,
}

impl std::fmt::Display for ComponentOwnerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "component owner operation failed: {self:?}")
    }
}

impl std::error::Error for ComponentOwnerError {}

struct PortRecord {
    owner: ComponentOwnerId,
    world: WorldInstanceId,
    lease: Arc<PortLease>,
}

/// Runtime-owned issuer and reservation state. All reservation changes happen
/// on the owner lane through ServerRuntime methods.
pub(crate) struct ComponentOwnerAuthority {
    issuer: Arc<Issuer>,
    worlds: BTreeMap<WorldInstanceId, Arc<WorldLease>>,
    ports: BTreeMap<u64, PortRecord>,
    leases_by_owner_world: BTreeMap<(u64, WorldInstanceId), u64>,
    reservations: BTreeMap<(WorldInstanceId, u64), BTreeSet<u64>>,
    reservation_count: usize,
    next_port: u64,
    active: bool,
}

impl ComponentOwnerAuthority {
    pub(crate) fn new(worlds: impl IntoIterator<Item = WorldInstanceId>) -> Self {
        Self {
            issuer: Arc::new(Issuer),
            worlds: worlds
                .into_iter()
                .map(|world| (world, Arc::new(WorldLease)))
                .collect(),
            ports: BTreeMap::new(),
            leases_by_owner_world: BTreeMap::new(),
            reservations: BTreeMap::new(),
            reservation_count: 0,
            next_port: 0,
            active: true,
        }
    }

    pub(crate) fn issue(
        &mut self,
        owner: ComponentOwnerId,
        world: WorldInstanceId,
    ) -> Result<ComponentOwnerPort, ComponentOwnerError> {
        if !self.active {
            return Err(ComponentOwnerError::Revoked);
        }
        let world_lease = self
            .worlds
            .get(&world)
            .ok_or(ComponentOwnerError::UnknownWorld)?;
        let owner_world = (owner.get(), world);
        if let Some(id) = self.leases_by_owner_world.get(&owner_world).copied() {
            if let Some(record) = self.ports.get(&id).filter(|record| {
                record.lease.active.load(Ordering::Acquire)
                    && record.owner == owner
                    && record.world == world
            }) {
                return Ok(ComponentOwnerPort {
                    owner,
                    world,
                    id,
                    issuer: Arc::downgrade(&self.issuer),
                    world_lease: Arc::downgrade(world_lease),
                    lease: Arc::downgrade(&record.lease),
                });
            }
            self.leases_by_owner_world.remove(&owner_world);
        }
        let id = self
            .next_port
            .checked_add(1)
            .ok_or(ComponentOwnerError::Revoked)?;
        self.next_port = id;
        let lease = Arc::new(PortLease {
            active: AtomicBool::new(true),
        });
        self.ports.insert(
            id,
            PortRecord {
                owner: owner.clone(),
                world,
                lease: lease.clone(),
            },
        );
        self.leases_by_owner_world.insert(owner_world, id);
        Ok(ComponentOwnerPort {
            owner,
            world,
            id,
            issuer: Arc::downgrade(&self.issuer),
            world_lease: Arc::downgrade(world_lease),
            lease: Arc::downgrade(&lease),
        })
    }

    pub(crate) fn validate(
        &self,
        port: &ComponentOwnerPort,
        owner: &ComponentOwnerId,
        world: WorldInstanceId,
    ) -> Result<(), ComponentOwnerError> {
        if !self.active {
            return Err(ComponentOwnerError::Revoked);
        }
        let issuer = port.issuer.upgrade().ok_or(ComponentOwnerError::Revoked)?;
        let world_lease = port
            .world_lease
            .upgrade()
            .ok_or(ComponentOwnerError::Revoked)?;
        let lease = port.lease.upgrade().ok_or(ComponentOwnerError::Revoked)?;
        let Some(current_world) = self.worlds.get(&world) else {
            return Err(ComponentOwnerError::UnknownWorld);
        };
        let Some(record) = self.ports.get(&port.id) else {
            return Err(ComponentOwnerError::Revoked);
        };
        if !Arc::ptr_eq(&issuer, &self.issuer)
            || !Arc::ptr_eq(&world_lease, current_world)
            || !Arc::ptr_eq(&lease, &record.lease)
            || port.owner != *owner
            || &record.owner != owner
            || record.world != world
            || port.world != world
        {
            return Err(ComponentOwnerError::ForeignPort);
        }
        if !lease.active.load(Ordering::Acquire) {
            return Err(ComponentOwnerError::Revoked);
        }
        Ok(())
    }

    pub(crate) fn reserve(
        &mut self,
        port: &ComponentOwnerPort,
        entity: u64,
    ) -> Result<(), ComponentOwnerError> {
        let owner = port.owner.clone();
        self.validate(port, &owner, port.world)?;
        let key = (port.world, entity);
        if self
            .reservations
            .get(&key)
            .is_some_and(|leases| leases.contains(&port.id))
        {
            return Ok(());
        }
        if self.reservation_count >= minimium_world::entity::MAX_PERSISTENT_ENTITIES {
            return Err(ComponentOwnerError::ReservationLimitExceeded);
        }
        self.reservations.entry(key).or_default().insert(port.id);
        self.reservation_count += 1;
        Ok(())
    }

    pub(crate) fn release_entity(
        &mut self,
        port: &ComponentOwnerPort,
        entity: u64,
    ) -> Result<bool, ComponentOwnerError> {
        let owner = port.owner.clone();
        self.validate(port, &owner, port.world)?;
        let key = (port.world, entity);
        let Some(leases) = self.reservations.get_mut(&key) else {
            return Ok(false);
        };
        if !leases.remove(&port.id) {
            return Ok(false);
        }
        self.reservation_count -= 1;
        if leases.is_empty() {
            self.reservations.remove(&key);
        }
        Ok(true)
    }

    pub(crate) fn release_port(
        &mut self,
        port: &ComponentOwnerPort,
    ) -> Result<(), ComponentOwnerError> {
        let owner = port.owner.clone();
        self.validate(port, &owner, port.world)?;
        if let Some(record) = self.ports.remove(&port.id) {
            record.lease.active.store(false, Ordering::Release);
        }
        self.leases_by_owner_world
            .remove(&(port.owner.get(), port.world));
        let mut released = 0usize;
        self.reservations.retain(|_, leases| {
            released += usize::from(leases.remove(&port.id));
            !leases.is_empty()
        });
        self.reservation_count -= released;
        Ok(())
    }

    pub(crate) fn is_reserved(&self, world: WorldInstanceId, entity: u64) -> bool {
        self.reservations
            .get(&(world, entity))
            .is_some_and(|leases| !leases.is_empty())
    }

    pub(crate) fn revoke_world(&mut self, world: WorldInstanceId) {
        self.worlds.remove(&world);
        let ports: BTreeSet<_> = self
            .ports
            .iter()
            .filter_map(|(id, record)| (record.world == world).then_some(*id))
            .collect();
        for id in &ports {
            if let Some(record) = self.ports.remove(id) {
                record.lease.active.store(false, Ordering::Release);
            }
        }
        let mut released = 0usize;
        self.reservations.retain(|(reserved_world, _), leases| {
            if *reserved_world == world {
                released += leases.len();
                return false;
            }
            let old_len = leases.len();
            leases.retain(|lease| !ports.contains(lease));
            released += old_len - leases.len();
            !leases.is_empty()
        });
        self.reservation_count -= released;
        self.leases_by_owner_world
            .retain(|(_, lease_world), _| *lease_world != world);
    }

    pub(crate) fn revoke_all(&mut self) {
        self.active = false;
        let worlds: Vec<_> = self.worlds.keys().copied().collect();
        for world in worlds {
            self.revoke_world(world);
        }
        self.ports.clear();
        self.leases_by_owner_world.clear();
        self.reservations.clear();
        self.reservation_count = 0;
    }
}

impl super::ServerRuntime {
    /// Issues a world-scoped owner port for a registered owner-only component.
    /// Modules receive neither this port nor mutable runtime access.
    pub fn component_owner_port(
        &mut self,
        component: ComponentTypeId,
        world: WorldInstanceId,
    ) -> Result<ComponentOwnerPort, ComponentOwnerError> {
        if self.shutdown_request.is_requested() {
            return Err(ComponentOwnerError::Revoked);
        }
        let descriptor = self
            .component_registry
            .descriptor(component)
            .ok_or(ComponentOwnerError::UnknownComponent)?;
        if descriptor.mutation_policy != minimium_world::entity::ComponentMutationPolicy::OwnerOnly
        {
            return Err(ComponentOwnerError::GeneralComponent);
        }
        if self.game.worlds.get(world).is_none() {
            return Err(ComponentOwnerError::UnknownWorld);
        }
        let owner = self
            .component_registry
            .owner_for_component(component)
            .ok_or(ComponentOwnerError::UnknownComponent)?;
        if !self.component_registry.is_module_owner(&owner) {
            return Err(ComponentOwnerError::ForeignPort);
        }
        self.component_owner_authority.issue(owner, world)
    }

    /// Reserves a world-scoped entity before its owner-only projection is published.
    pub fn reserve_component_entity(
        &mut self,
        port: &ComponentOwnerPort,
        entity: EntityId,
    ) -> Result<(), ComponentOwnerError> {
        if self.shutdown_request.is_requested() {
            return Err(ComponentOwnerError::Revoked);
        }
        self.validate_component_owner_port(port)?;
        if self
            .game
            .worlds
            .get(port.world())
            .is_none_or(|world| !world.entities.contains(entity))
        {
            return Err(ComponentOwnerError::Validation);
        }
        self.component_owner_authority.reserve(port, entity.get())
    }

    /// Releases an entity reservation held by this owner port.
    pub fn release_component_entity(
        &mut self,
        port: &ComponentOwnerPort,
        entity: EntityId,
    ) -> Result<bool, ComponentOwnerError> {
        self.validate_component_owner_port(port)?;
        self.component_owner_authority
            .release_entity(port, entity.get())
    }

    /// Revokes this port and every clone, clearing its active entity reservations.
    pub fn release_component_owner_port(
        &mut self,
        port: &ComponentOwnerPort,
    ) -> Result<(), ComponentOwnerError> {
        self.validate_component_owner_port(port)?;
        self.component_owner_authority.release_port(port)
    }

    fn validate_component_owner_port(
        &self,
        port: &ComponentOwnerPort,
    ) -> Result<(), ComponentOwnerError> {
        if !self.component_registry.contains_owner(port.owner()) {
            return Err(ComponentOwnerError::ForeignPort);
        }
        self.component_owner_authority
            .validate(port, port.owner(), port.world())
    }

    pub(super) fn initialize_component_owner_ports(&mut self) -> std::io::Result<()> {
        let owner = self
            .component_registry
            .owner_for_component(minimium_world::impact::BODY_COMPONENT)
            .ok_or_else(|| std::io::Error::other("impact component owner is not registered"))?;
        let worlds: Vec<_> = self.game.worlds.iter().map(|(world, _)| world).collect();
        for world in worlds {
            let port = self
                .component_owner_authority
                .issue(owner.clone(), world)
                .map_err(std::io::Error::other)?;
            for body in self.impacts.bodies().filter(|body| body.world == world) {
                self.component_owner_authority
                    .reserve(&port, body.id)
                    .map_err(std::io::Error::other)?;
            }
            self.impacts.owner_ports.insert(world, port);
        }
        Ok(())
    }

    pub(super) fn impact_owner_port(
        &self,
        world: WorldInstanceId,
    ) -> Result<&ComponentOwnerPort, ComponentOwnerError> {
        self.impacts
            .owner_ports
            .get(&world)
            .ok_or(ComponentOwnerError::UnknownWorld)
    }

    pub(super) fn reserve_impact_entity(
        &mut self,
        world: WorldInstanceId,
        entity: EntityId,
    ) -> Result<(), ComponentOwnerError> {
        let port = self
            .impacts
            .owner_ports
            .get(&world)
            .ok_or(ComponentOwnerError::UnknownWorld)?
            .clone();
        self.component_owner_authority.reserve(&port, entity.get())
    }

    pub(super) fn release_impact_entity(
        &mut self,
        world: WorldInstanceId,
        entity: EntityId,
    ) -> Result<bool, ComponentOwnerError> {
        let port = self
            .impacts
            .owner_ports
            .get(&world)
            .ok_or(ComponentOwnerError::UnknownWorld)?
            .clone();
        self.component_owner_authority
            .release_entity(&port, entity.get())
    }
}
