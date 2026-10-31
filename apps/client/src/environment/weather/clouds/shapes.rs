//! Budget-conserving hierarchy. Every cut through the tree has the same mass.
use super::{
    density, lifecycle,
    types::{random, CloudCluster, CloudId, CloudLobe},
};
use crate::environment::weather::wind::{self, EARTH_RADIUS};
use glam::{DMat3, DQuat, DVec3};

pub(crate) const DETAIL_DEPTH: u8 =
    super::coordinates::MAX_LEVEL - super::coordinates::CLUSTER_LEVEL;

pub(crate) fn lobes(cluster: &CloudCluster, depth: u8) -> Vec<CloudLobe> {
    let mut result = Vec::new();
    if depth == 0 {
        emit(cluster, cluster.id, 0, 1.0, 1.2, &mut result);
        return result;
    }
    for (population, weight, age) in lifecycle::parts(
        cluster.seconds,
        cluster.id.hash(),
        cluster.id.layer,
        cluster.convection,
    ) {
        if weight <= 0.0 {
            continue;
        }
        let mut id = cluster.id;
        id.part_population = population;
        emit(
            cluster,
            id,
            depth.min(DETAIL_DEPTH),
            weight,
            age,
            &mut result,
        );
    }
    result
}

fn emit(
    c: &CloudCluster,
    id: CloudId,
    depth: u8,
    weight: f64,
    age: f64,
    result: &mut Vec<CloudLobe>,
) {
    if depth > 0 {
        for branch in 0..2 {
            let mut child = id;
            child.path = id.path * 2 + branch;
            emit(c, child, depth - 1, weight * 0.5, age, result);
        }
        return;
    }
    let key = id.hash();
    let level = 31 - id.path.leading_zeros();
    let count = (1_u32 << level) as f64;
    let index = f64::from(id.path - (1 << level));
    let angle = index * 2.399963229728653 + random(key, 10) * 1.1;
    let spread = (1.0 - 1.0 / count).sqrt();
    let (east, north) = wind::basis(c.up);
    let along = c.wind.try_normalize().unwrap_or(east);
    let across = along.cross(c.up).normalize();
    let orientation = DQuat::from_mat3(&DMat3::from_cols(along, c.up, across));
    let vertical_growth = wind::smooth(0.0, 1.4, age);
    let vertical = (0.35 + random(key, 11) * 0.6) * c.convection * spread;
    let tower = vertical * c.depth_m * vertical_growth;
    let horizontal = c.width_m * spread * (0.2 + random(key, 12) * 0.35);
    let anvil = wind::smooth(0.55, 0.95, vertical) * c.convection;
    let offset = (east * angle.cos() + north * angle.sin()) * horizontal
        + c.shear * vertical
        + along * (anvil * c.width_m * 0.4 + c.width_m * 0.15 * (age - 1.5) * spread);
    let up = wind::displace(c.up, offset);
    let size = 1.0 / count.powf(0.23);
    let x = (c.width_m * size * (1.0 + c.fibrous * 0.4 + anvil * 0.5)).min(25000.0);
    let z = (c.width_m * size * (0.75 - c.fibrous * 0.5)).min(25000.0);
    let y = (c.depth_m * (0.65 - vertical * 0.2) * size).max(40.0);
    let center_height = (y * 0.35 + tower).min((c.depth_m - y).max(0.0));
    let mut lobe = CloudLobe {
        id,
        center: up * (EARTH_RADIUS + c.base_m + center_height),
        axes: DVec3::new(x, y, z),
        orientation,
        density: 0.0,
        base_normal: c.up,
        base_offset: EARTH_RADIUS + c.base_m,
        mass: c.mass * weight,
        layer: c.id.layer,
        storm: c.convection * wind::smooth(2500.0, 6500.0, c.depth_m),
        rain_potential: c.rain_potential,
    };
    lobe.density = lobe.mass / density::volume(&lobe).max(1.0);
    result.push(lobe);
}
