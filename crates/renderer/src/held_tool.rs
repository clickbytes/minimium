//! Bounded CPU description of a camera-local handheld viewmodel.

use glam::{EulerRot, Mat4, Quat, Vec3};
use std::time::Duration;

pub const DIG_SWING_DURATION: Duration = Duration::from_millis(480);

/// Cosmetic camera-local pose, independent of tool identity and gameplay clocks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeldToolPose(Mat4);

impl Default for HeldToolPose {
    fn default() -> Self {
        Self(Mat4::IDENTITY)
    }
}

impl HeldToolPose {
    /// Wind-up, strike, short recoil, and recovery around the common hand grip.
    /// Sampling by elapsed time gives the same pose at any redraw frequency.
    pub fn digging(elapsed: Duration) -> Self {
        if elapsed >= DIG_SWING_DURATION {
            return Self::default();
        }
        let phase = elapsed.as_secs_f32() / DIG_SWING_DURATION.as_secs_f32();
        let keys = [
            (0.0, Vec3::ZERO, Vec3::ZERO),
            (
                0.18,
                Vec3::new(0.045, 0.025, 0.03),
                Vec3::new(0.18, 0.08, -0.12),
            ),
            (
                0.40,
                Vec3::new(-0.10, -0.08, -0.08),
                Vec3::new(-0.85, -0.15, 0.12),
            ),
            (
                0.48,
                Vec3::new(-0.085, -0.065, -0.06),
                Vec3::new(-0.72, -0.12, 0.10),
            ),
            (1.0, Vec3::ZERO, Vec3::ZERO),
        ];
        let segment = keys.windows(2).find(|pair| phase <= pair[1].0).unwrap();
        let t = (phase - segment[0].0) / (segment[1].0 - segment[0].0);
        let t = t * t * (3.0 - 2.0 * t);
        let offset = segment[0].1.lerp(segment[1].1, t);
        let angles = segment[0].2.lerp(segment[1].2, t);
        let pivot = Vec3::new(0.34, -0.32, -0.55);
        let rotation = Quat::from_euler(EulerRot::XYZ, angles.x, angles.y, angles.z);
        Self(
            Mat4::from_translation(pivot + offset)
                * Mat4::from_quat(rotation)
                * Mat4::from_translation(-pivot),
        )
    }

    pub fn transform(self) -> [[f32; 4]; 4] {
        self.0.to_cols_array_2d()
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct HeldToolVertex {
    pub position: [f32; 3],
    /// Linear RGB, written after atmospheric composition with unit pre-exposure.
    pub color: [f32; 3],
}

pub const MAX_HELD_TOOL_MODELS: usize = 8;
pub const MAX_HELD_TOOL_MODEL_VERTICES: usize = 512;
pub const MAX_HELD_TOOL_TOTAL_VERTICES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeldToolModelError {
    InvalidVertices,
    Capacity,
    InvalidCatalog,
}

impl std::fmt::Display for HeldToolModelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidVertices => formatter.write_str("invalid held-tool model vertices"),
            Self::Capacity => formatter.write_str("held-tool model catalog exceeds its bounds"),
            Self::InvalidCatalog => formatter.write_str("held-tool model catalog is empty"),
        }
    }
}

impl std::error::Error for HeldToolModelError {}

#[derive(Clone, Debug, PartialEq)]
pub struct HeldToolModel {
    vertices: Box<[HeldToolVertex]>,
}

impl HeldToolModel {
    pub fn new(vertices: Vec<HeldToolVertex>) -> Result<Self, HeldToolModelError> {
        if vertices.len() > MAX_HELD_TOOL_MODEL_VERTICES {
            return Err(HeldToolModelError::Capacity);
        }
        if vertices.is_empty()
            || vertices.len() % 3 != 0
            || vertices.iter().any(|vertex| {
                vertex.position.iter().any(|value| !value.is_finite())
                    || vertex.color.iter().any(|value| !value.is_finite())
            })
        {
            return Err(HeldToolModelError::InvalidVertices);
        }
        Ok(Self {
            vertices: vertices.into_boxed_slice(),
        })
    }

    pub fn vertices(&self) -> &[HeldToolVertex] {
        &self.vertices
    }
}
