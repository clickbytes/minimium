//! CPU-only diagnostic selection, also parsed by headless command frontends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum TerrainDebug {
    #[default]
    Off,
    Albedo,
    Normals,
    Depth,
    Shadow,
    BaseColor,
    Coverage,
    Habitat,
    Source,
}
