/// Decision of the authoritative movement integrator for a requested jump.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JumpResolution {
    Applied,
    NotGrounded,
    Swimming,
    FlightMode,
}

impl JumpResolution {
    pub(super) fn at_ground(requested: bool, grounded: bool, swimming: bool) -> Option<Self> {
        requested.then(|| {
            if swimming {
                Self::Swimming
            } else if grounded {
                Self::Applied
            } else {
                Self::NotGrounded
            }
        })
    }
}
