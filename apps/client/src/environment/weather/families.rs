//! Phenomenological families, reference Earth dimensions and altitude envelopes.
use super::{random, Cloud, Conditions, WeatherModel};
use glam::DVec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum CloudKind {
    Cumulus,
    Congestus,
    Cumulonimbus,
    Stratus,
    Stratocumulus,
    Nimbostratus,
    Altocumulus,
    Altostratus,
    Lenticularis,
    Cirrus,
    Cirrocumulus,
    Cirrostratus,
}

impl CloudKind {
    pub const ALL: [Self; 12] = [
        Self::Cumulus,
        Self::Congestus,
        Self::Cumulonimbus,
        Self::Stratus,
        Self::Stratocumulus,
        Self::Nimbostratus,
        Self::Altocumulus,
        Self::Altostratus,
        Self::Lenticularis,
        Self::Cirrus,
        Self::Cirrocumulus,
        Self::Cirrostratus,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Cumulus => "cumulus",
            Self::Congestus => "cumulus-congestus",
            Self::Cumulonimbus => "cumulonimbus",
            Self::Stratus => "stratus",
            Self::Stratocumulus => "stratocumulus",
            Self::Nimbostratus => "nimbostratus",
            Self::Altocumulus => "altocumulus",
            Self::Altostratus => "altostratus",
            Self::Lenticularis => "lenticularis",
            Self::Cirrus => "cirrus",
            Self::Cirrocumulus => "cirrocumulus",
            Self::Cirrostratus => "cirrostratus",
        }
    }

    pub fn morph_seconds(self) -> f64 {
        match self {
            Self::Cumulus | Self::Congestus | Self::Cumulonimbus => 12.0,
            Self::Cirrus | Self::Cirrocumulus => 18.0,
            _ => 24.0,
        }
    }

    pub fn select(c: Conditions, layer: usize, id: u64) -> Self {
        let r = random(id ^ 0x63f1);
        let convection =
            ((c.temperature_c - 5.0) / 25.0).clamp(0.0, 1.0) * c.humidity + c.uplift.max(0.0) * 0.4;
        match layer {
            0 if convection > 0.65 && c.humidity > 0.65 && r < 0.16 => Self::Cumulonimbus,
            0 if convection > 0.45 && r < 0.35 => Self::Congestus,
            0 if c.humidity > 0.68 && convection < 0.4 && r < 0.22 => Self::Nimbostratus,
            0 if convection < 0.3 && r < 0.5 => Self::Stratus,
            0 if r < 0.7 => Self::Stratocumulus,
            0 => Self::Cumulus,
            1 if c.uplift > 0.18 && r < 0.35 => Self::Lenticularis,
            1 if c.humidity > 0.6 && r < 0.65 => Self::Altostratus,
            1 => Self::Altocumulus,
            _ if r < 0.45 || c.layers[2].coverage < 0.3 => Self::Cirrus,
            _ if r < 0.75 => Self::Cirrocumulus,
            _ => Self::Cirrostratus,
        }
    }
}

/// Returns the allowed base/top. Convective and nimbostratus columns span floors.
pub(super) fn configure(model: &WeatherModel, c: Conditions, cloud: &mut Cloud) -> (f64, f64) {
    use CloudKind::*;
    let kind = CloudKind::select(c, cloud.layer, cloud.id);
    let layer = c.layers[cloud.layer];
    let r = random(cloud.id ^ 0xf37a);
    let (width, height, aspect, optical) = match kind {
        Cumulus => (2800.0, 1100.0, 0.8, 1.3),
        Congestus => (3300.0, 4300.0, 0.85, 2.2),
        Cumulonimbus => (6500.0, 8500.0, 0.85, 3.2),
        Stratus => (7000.0, 650.0, 0.85, 0.8),
        Stratocumulus => (6500.0, 1250.0, 0.85, 1.2),
        Nimbostratus => (8500.0, 3400.0, 0.9, 2.4),
        Altocumulus => (6000.0, 900.0, 0.7, 0.7),
        Altostratus => (8500.0, 1200.0, 0.85, 0.65),
        Lenticularis => (4500.0, 1100.0, 0.5, 0.9),
        Cirrus => (6500.0, 650.0, 0.35, 0.25),
        Cirrocumulus => (6500.0, 550.0, 0.7, 0.3),
        Cirrostratus => (9500.0, 700.0, 0.85, 0.22),
    };
    let ceiling = match kind {
        Congestus => (cloud.base_m + 6500.0 * model.scale).min(10000.0 * model.scale),
        Cumulonimbus => 12000.0 * model.scale,
        Nimbostratus => 7000.0 * model.scale,
        _ => layer.ceiling_m,
    };
    let width = width * (0.75 + r * 0.5);
    let height = (height * (0.75 + c.humidity * 0.5)).min((ceiling - cloud.base_m) / model.scale);
    let raw = DVec3::new(width, height.max(100.0), width * aspect) * model.scale;
    // Dyadic lattice: choose a root size and snap the support inward. At least
    // two vertical cells preserve thin layers even when detail is budget-limited.
    let root = raw.max_element();
    let min_level = ((2.0 * root / raw.min_element()).log2().ceil() as u8).clamp(2, 6);
    let cell = root / f64::from(1_u32 << min_level);
    cloud.kind = kind;
    cloud.size_m = ((raw / cell) + DVec3::splat(1e-10))
        .floor()
        .max(DVec3::splat(1.0))
        * cell;
    cloud.min_level = min_level;
    cloud.max_level = 6;
    cloud.opacity = (optical * (0.4 + layer.coverage * 0.6)) as f32;
    (layer.max_base_m, ceiling)
}
