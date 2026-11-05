use super::{intersects_player, oriented_capsule_intersects};
use crate::world::Aabb;

#[test]
fn capsule_collision_follows_radial_up_instead_of_world_y() {
    let collider = Aabb::from_center_size([0.95, 0.15, 0.0], [0.4, 0.1, 0.2]);
    let position = [0.0, 0.0, 0.0];

    assert!(!intersects_player(position, collider));
    assert!(oriented_capsule_intersects(
        position,
        [1.0, 0.0, 0.0],
        collider
    ));
}

#[test]
fn capsule_stays_clear_of_boxes_outside_its_radius() {
    let collider = Aabb::from_center_size([0.4, 0.0, 0.4], [0.2, 2.0, 0.2]);
    assert!(!intersects_player([0.0, 0.0, 0.0], collider));
}

#[test]
fn standing_contact_with_a_floor_is_not_penetration() {
    let floor = Aabb::from_center_size([0.0, -0.15, 0.0], [20.0, 0.3, 20.0]);
    assert!(!intersects_player([0.0, 0.0, 0.0], floor));
}
