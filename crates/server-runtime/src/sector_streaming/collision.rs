use super::SectorPayload;
use crate::world_registry::WorldRegistry;
use minimium_sectors::{SectorManager, SectorTicket};
use minimium_world::world::{Aabb, MapBox, MapTriangle, WorldInstanceId};
use std::collections::HashSet;

pub(super) fn installed_tickets(sectors: &SectorManager<SectorPayload>) -> HashSet<SectorTicket> {
    sectors.installed().map(|(ticket, _, _)| ticket).collect()
}

pub(super) fn changed_instances(
    installed_before: &HashSet<SectorTicket>,
    installed_after: &HashSet<SectorTicket>,
) -> HashSet<WorldInstanceId> {
    installed_before
        .symmetric_difference(installed_after)
        .map(|ticket| ticket.instance())
        .collect()
}

pub(super) fn update_changed_instances(
    worlds: &mut WorldRegistry,
    sectors: &SectorManager<SectorPayload>,
    changed: HashSet<WorldInstanceId>,
) {
    for instance in changed {
        install_instance_collision(worlds, sectors, instance);
    }
}

fn install_instance_collision(
    worlds: &mut WorldRegistry,
    sectors: &SectorManager<SectorPayload>,
    instance: WorldInstanceId,
) {
    let (colliders, surfaces) = prepare_collision_geometry(sectors, instance);
    if let Some(owner) = worlds.get_mut(instance) {
        owner.world.replace_streamed_collision(colliders, surfaces);
    }
}

fn prepare_collision_geometry(
    sectors: &SectorManager<SectorPayload>,
    instance: WorldInstanceId,
) -> (Vec<Aabb>, Vec<MapTriangle>) {
    let mut colliders = Vec::new();
    let mut seen_boxes = HashSet::new();
    let mut surfaces = Vec::new();
    let mut seen_triangles = HashSet::new();
    for (ticket, _, payload) in sectors.installed() {
        if ticket.instance() != instance {
            continue;
        }
        for map_box in &payload.boxes {
            if map_box.solid && seen_boxes.insert(box_geometry_key(*map_box)) {
                colliders.push(Aabb::from_center_size(map_box.center, map_box.size));
            }
        }
        for triangle in &payload.triangles {
            if triangle.solid && seen_triangles.insert(triangle_geometry_key(*triangle)) {
                surfaces.push(*triangle);
            }
        }
    }
    (colliders, surfaces)
}

fn box_geometry_key(map_box: MapBox) -> [u32; 6] {
    [
        map_box.center[0].to_bits(),
        map_box.center[1].to_bits(),
        map_box.center[2].to_bits(),
        map_box.size[0].to_bits(),
        map_box.size[1].to_bits(),
        map_box.size[2].to_bits(),
    ]
}

fn triangle_geometry_key(triangle: MapTriangle) -> [u32; 9] {
    let [a, b, c] = triangle.vertices;
    [
        a[0].to_bits(),
        a[1].to_bits(),
        a[2].to_bits(),
        b[0].to_bits(),
        b[1].to_bits(),
        b[2].to_bits(),
        c[0].to_bits(),
        c[1].to_bits(),
        c[2].to_bits(),
    ]
}
