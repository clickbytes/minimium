//! Instanced face data consumed by the regional cloud pipelines.

/// Compact oriented cloud face: center, U extent and V extent.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CloudFace {
    pub center: [f32; 4],
    pub u: [f32; 4],
    pub v: [f32; 4],
}

/// Maximum number of cloud face instances reserved by one client renderer.
pub const MAX_CLOUD_FACES: usize = 131_072;

const _: () = assert!(std::mem::size_of::<CloudFace>() == 48);
