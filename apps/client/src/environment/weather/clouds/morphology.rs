use super::{atmosphere::CloudColumn, types::CloudFamily};
use crate::environment::weather::wind::smooth;

pub(super) struct Morphology {
    pub(super) family: CloudFamily,
    pub(super) convection: f64,
    pub(super) fibrous: f64,
    pub(super) width: f64,
    pub(super) base: f64,
    pub(super) depth: f64,
    pub(super) optical: f64,
}

pub(super) fn evaluate(column: &CloudColumn, layer: usize) -> Morphology {
    let p = column[3 + layer].as_dvec4();
    let convection = if layer == 0 {
        smooth(0.1, 0.85, f64::from(column[2].z))
    } else {
        0.0
    };
    let coverage = f64::from(column[0][layer]);
    let fibrous = f64::from(layer == 2);
    let base = p.x * 20000.0;
    let depth = (p.y * 20000.0 - base).clamp(80.0, 14000.0);
    // Family is a diagnostic label; geometry uses the continuous parameters.
    let family = if layer == 2 {
        CloudFamily::Cirrus
    } else if convection > 0.75 && depth > 5000.0 {
        CloudFamily::Storm
    } else if column[0].w > 0.15 && coverage > 0.65 {
        CloudFamily::Nimbostratus
    } else if convection < 0.2 {
        CloudFamily::Stratus
    } else if coverage > 0.65 {
        CloudFamily::Stratocumulus
    } else {
        CloudFamily::Cumulus
    };
    Morphology {
        family,
        convection,
        fibrous,
        base,
        depth,
        width: 23000.0 - convection * 15000.0,
        optical: (p.z * depth * if layer == 2 { 0.0012 } else { 0.005 }).clamp(0.0, 18.0),
    }
}
