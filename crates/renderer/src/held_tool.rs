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
    vertices: Vec<HeldToolVertex>,
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
        Ok(Self { vertices })
    }

    pub fn vertices(&self) -> &[HeldToolVertex] {
        &self.vertices
    }
}
