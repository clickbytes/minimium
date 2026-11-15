//! Bounded CPU description of a camera-local handheld viewmodel.

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

#[derive(Clone, Debug, PartialEq)]
pub struct HeldToolModel {
    vertices: Vec<HeldToolVertex>,
}

impl HeldToolModel {
    pub fn new(vertices: Vec<HeldToolVertex>) -> Result<Self, HeldToolModelError> {
        if vertices.is_empty()
            || vertices.len() % 3 != 0
            || vertices.iter().any(|vertex| {
                vertex.position.iter().any(|value| !value.is_finite())
                    || vertex.color.iter().any(|value| !value.is_finite())
            })
        {
            return Err(HeldToolModelError::InvalidVertices);
        }
        if vertices.len() > MAX_HELD_TOOL_MODEL_VERTICES {
            return Err(HeldToolModelError::Capacity);
        }
        Ok(Self { vertices })
    }

    pub fn vertices(&self) -> &[HeldToolVertex] {
        &self.vertices
    }
}
