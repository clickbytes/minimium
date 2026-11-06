//! CPU-only atmosphere diagnostic selection for graphical and terminal frontends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum AtmosphereDebug {
    #[default]
    Off,
    Transmission,
    Scattering,
    OpticalDepth,
    Distance,
    Fog,
}
