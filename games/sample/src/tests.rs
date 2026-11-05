use crate::generate_demo_worlds;

#[test]
fn demo_worlds_are_distinct_and_portals_form_a_round_trip() {
    let [planet, dungeon] = generate_demo_worlds().unwrap();
    let outward = planet.portal.unwrap();
    let inward = dungeon.portal.unwrap();
    assert_ne!(planet.instance, dungeon.instance);
    assert_eq!(outward.target_instance, dungeon.instance);
    assert_eq!(inward.target_instance, planet.instance);
    assert_eq!(inward.destination, outward.position);
}
