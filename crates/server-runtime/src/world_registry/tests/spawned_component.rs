use super::*;
use minimium_world::entity::ComponentError;

#[test]
fn spawned_component_entity_uses_the_budgeted_entity_effect_path() {
    let worlds = generate_demo_worlds().unwrap();
    let instance = worlds[0].instance;
    let mut owner = registry_from_sample(worlds.to_vec());
    let mut budget = crate::storage::SnapshotBudget::default();
    let mut components = ComponentRegistry::default();
    let kind = ComponentTypeId::new(1).unwrap();
    let schema = SchemaVersion::new(1).unwrap();
    components
        .register(ComponentDescriptor {
            id: kind,
            name: "demo.marker",
            schema,
            validate_payload: |bytes| {
                (bytes.len() == 3)
                    .then_some(())
                    .ok_or(ComponentError::InvalidPayload)
            },
            replication: ReplicationPolicy::ServerOnly,
            visibility: minimium_world::entity::ReplicationVisibility::Global,
        })
        .unwrap();
    let sector = SectorKey::planet(PlanetFace::PositiveY, 3, 4).unwrap();
    let staged = owner
        .stage_entity_effect_batch(
            &[crate::modules::WorldEffect::SpawnEntity {
                world: instance,
                sector,
                components: vec![crate::modules::ComponentValue {
                    kind,
                    schema,
                    bytes: vec![1, 2, 3],
                }],
            }],
            &components,
            &crate::runtime::ProtectedImpactEntities::test_empty(),
        )
        .unwrap();
    let reservation = budget.prepare_entity_changes(staged.changes()).unwrap();
    staged.commit(reservation);
    let entity = owner.get(instance).unwrap().entities.iter().next().unwrap();
    assert_eq!(
        owner
            .get(instance)
            .unwrap()
            .entities
            .get(entity, kind)
            .unwrap()
            .bytes,
        vec![1, 2, 3]
    );
    assert!(owner.sector_states().is_empty());
}
