//! Bounded data-only terrain material packages. No executable mod content.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
pub const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;
pub const ROLES: [&str; 6] = ["sand", "soil", "grass", "litter", "rock", "snow"];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialVariant {
    pub edge: u32,
    pub compressed: bool,
    /// Six-layer arrays: sRGB albedo, tangent normal, roughness/AO/height/reflectance.
    pub maps: [MaterialFile; 3],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialManifest {
    pub version: u32,
    pub roles: [String; 6],
    pub meters_per_tile: [f32; 6],
    pub height_blend: f32,
    pub variants: [MaterialVariant; 2],
}

impl MaterialManifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err("material manifest too large".into());
        }
        let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
        let manifest: Self = ron::from_str(text).map_err(|e| e.to_string())?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.roles.iter().map(String::as_str).ne(ROLES)
            || self
                .meters_per_tile
                .iter()
                .any(|v| !v.is_finite() || !(0.05..=100.0).contains(v))
            || !self.height_blend.is_finite()
            || !(0.01..=1.0).contains(&self.height_blend)
        {
            return Err("invalid material roles, version or blending parameters".into());
        }
        let mut names = std::collections::HashSet::new();
        for (variant, edge) in self.variants.iter().zip([4096, 2048]) {
            if variant.edge != edge || variant.compressed != (edge == 4096) {
                return Err("expected BC 4096 and RGBA 2048 variants".into());
            }
            let mut bytes = 0_u64;
            for file in &variant.maps {
                if file.name.is_empty()
                    || file.name.len() > 128
                    || !file
                        .name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
                    || file.name.starts_with('.')
                    || !file.name.ends_with(".ktx2")
                    || !names.insert(file.name.as_str())
                    || file.bytes == 0
                    || file.bytes > MAX_PACKAGE_BYTES
                    || decode_hash(&file.sha256).is_err()
                {
                    return Err("invalid material file".into());
                }
                bytes = bytes
                    .checked_add(file.bytes)
                    .ok_or("material size overflow")?;
            }
            if bytes > MAX_PACKAGE_BYTES {
                return Err("material download budget exceeded".into());
            }
        }
        Ok(())
    }
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn decode_hash(text: &str) -> Result<[u8; 32], String> {
    if text.len() != 64
        || !text
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("invalid SHA-256".into());
    }
    let mut bytes = [0; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}
