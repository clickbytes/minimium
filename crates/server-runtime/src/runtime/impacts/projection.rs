use super::*;

pub(in crate::runtime) fn prepare(
    worlds: &WorldRegistry,
    components: &minimium_world::entity::ComponentRegistry,
    authority: &super::ComponentOwnerAuthority,
    port: &super::ComponentOwnerPort,
    body: BodyState,
    restoring: bool,
) -> io::Result<(
    minimium_world::entity::EntityStore,
    Arc<RemnantSnapshot>,
    bool,
)> {
    let owner = components
        .owner_for_component(BODY_COMPONENT)
        .ok_or_else(|| io::Error::other("impact component owner is not registered"))?;
    if !components.owns(port.owner(), BODY_COMPONENT) {
        return Err(io::Error::other("impact owner port is invalid"));
    }
    authority
        .validate(port, &owner, body.world)
        .map_err(io::Error::other)?;
    let owner = worlds
        .get(body.world)
        .ok_or_else(|| io::Error::other("impact world missing"))?;
    if owner.manifest.generation().seed().get() != body.world_seed
        || owner
            .manifest
            .planet()
            .is_none_or(|p| (p.radius_mm as f64 / 1000.0 - body.planet_radius_m).abs() > 0.001)
    {
        return Err(io::Error::other("impact planet mismatch"));
    }
    let mut entities = owner.entities.clone();
    let mut remnants = owner.remnants.clone();
    let entity = EntityId::new(body.id).ok_or_else(|| io::Error::other("invalid body ID"))?;
    if !entities.contains(entity) {
        entities
            .restore(entity)
            .map_err(|e| io::Error::other(format!("impact entity: {e:?}")))?;
    } else if restoring
        && owner.entities.get(entity, BODY_COMPONENT).is_none()
        && owner
            .entities
            .iter_components()
            .any(|(id, _, _)| id == entity)
    {
        return Err(io::Error::other(
            "saved impact ID conflicts with another entity",
        ));
    }
    let bytes = body.encode();
    entities
        .insert(components, entity, BODY_COMPONENT, BODY_SCHEMA, &bytes)
        .map_err(|e| io::Error::other(format!("impact component: {e:?}")))?;
    if body.crater().is_some() {
        Arc::make_mut(&mut remnants)
            .set(body)
            .map_err(io::Error::other)?;
    }
    let changed = remnants.revision() != owner.remnants.revision();
    Ok((entities, remnants, changed))
}

pub(in crate::runtime) fn publish(
    worlds: &mut WorldRegistry,
    components: &minimium_world::entity::ComponentRegistry,
    authority: &super::ComponentOwnerAuthority,
    port: &super::ComponentOwnerPort,
    body: BodyState,
    restoring: bool,
) -> io::Result<bool> {
    let (entities, remnants, changed) =
        prepare(worlds, components, authority, port, body, restoring)?;
    let entity_membership_changed = worlds.get(body.world).is_some_and(|owner| {
        owner.entities.membership_revision() != entities.membership_revision()
    });
    let next_membership_revision = if entity_membership_changed {
        Some(
            worlds
                .next_entity_membership_revision()
                .ok_or_else(|| io::Error::other("entity membership revision exhausted"))?,
        )
    } else {
        None
    };
    let owner = worlds.get_mut(body.world).unwrap();
    owner.entities = entities;
    owner.remnants = remnants;
    if let Some(revision) = next_membership_revision {
        worlds.entity_membership_revision = revision;
    }
    Ok(changed)
}
