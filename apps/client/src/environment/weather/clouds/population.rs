use super::{
    coordinates::Cell,
    field::CloudFieldSnapshot,
    lifecycle, morphology, motion,
    spatial::Root,
    types::{random, CloudCluster, CloudId},
};
use crate::environment::weather::{transport::COHORT_SECONDS, wind};

pub(super) fn cluster(
    field: &CloudFieldSnapshot,
    root: &Root,
    cell: Cell,
    candidate: u8,
) -> Option<CloudCluster> {
    let id = CloudId {
        seed: field.seed,
        cell,
        population: root.population,
        candidate: candidate + u8::from(root.anchored) * 2,
        layer: root.layer,
        part_population: 0,
        path: 1,
    };
    let key = id.hash();
    let source = cell.direction(random(key, 1), random(key, 2));
    let moving = root.rotation * source;
    let column = field.atmosphere.column(moving, field.blend);
    let layer = usize::from(root.layer);
    let m = morphology::evaluate(&column, layer);
    let age = field.seconds / COHORT_SECONDS - root.population as f64;
    let growth = lifecycle::weight(age, random(key, 3) * 0.35);
    let coverage = f64::from(column[0][layer]);
    // Smooth activation keeps weather changes from replacing whole geometries.
    let supply = wind::smooth(
        random(key, 4) * 0.85,
        random(key, 4) * 0.85 + 0.15,
        coverage,
    );
    if growth * supply * m.optical < 0.00001 {
        return None;
    }
    let (flow, shear) = motion::deformation(&field.flow, moving, m.base, m.depth);
    let (east, north) = wind::basis(moving);
    let slope = (flow.dot(east) * (f64::from(column[7].z) - 0.5)
        + flow.dot(north) * (f64::from(column[7].w) - 0.5))
        .max(0.0);
    let anchored = wind::smooth(0.1, 1.5, slope)
        * (1.0 - m.convection)
        * wind::smooth(0.65, 0.95, f64::from(column[1].x));
    let share = if root.anchored {
        anchored * 0.8
    } else {
        1.0 - anchored * 0.8
    };
    if share < 0.00001 {
        return None;
    }
    let ground = f64::from(column[2].w) * 20000.0;
    let shore = [
        f64::from(column[7].x) * 2.0 - 1.0,
        f64::from(column[7].y) * 2.0 - 1.0,
    ];
    let coastal = field
        .flow
        .coastal_displacement(moving, m.base - ground, shore, 1800.0);
    let up = wind::displace(moving, coastal);
    let flow = field
        .flow
        .velocity(up, m.base + m.depth * 0.5, ground, shore);
    let width = m.width * (0.7 + random(key, 5) * 0.3);
    Some(CloudCluster {
        id,
        family: m.family,
        up,
        base_m: m.base,
        depth_m: m.depth * (0.65 + 0.35 * growth),
        width_m: width,
        convection: m.convection,
        fibrous: m.fibrous,
        mass: m.optical * std::f64::consts::PI * width * width * growth * supply * share * 0.45,
        wind: flow,
        shear,
        rain_potential: f64::from(column[0].w) * 32.0,
        seconds: field.seconds,
    })
}
