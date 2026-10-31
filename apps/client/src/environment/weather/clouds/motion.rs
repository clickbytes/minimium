use crate::environment::weather::transport::FlowFrame;
use glam::{DQuat, DVec3};

/// A root is transported as a rigid spherical patch. Inverting the existing
/// bounded UTC maps reconstructs its center; descendants share that rotation.
/// The spatial index stores the rotated bound, so no search assumes birth cells.
pub(super) fn rotation(frame: &FlowFrame, source: DVec3, height: f64, cohort: usize) -> DQuat {
    let mut target = source;
    for _ in 0..4 {
        let previous = frame.origin(target, height, cohort);
        target = (DQuat::from_rotation_arc(previous, source) * target).normalize();
    }
    DQuat::from_rotation_arc(source, target)
}

pub(super) fn deformation(frame: &FlowFrame, up: DVec3, base: f64, depth: f64) -> (DVec3, DVec3) {
    let bottom = frame.wind.velocity(up, base);
    let top = frame.wind.velocity(up, base + depth);
    let shear = crate::environment::weather::transport::lean(up, bottom, top, depth, 1.0);
    ((bottom + top) * 0.5, shear)
}
