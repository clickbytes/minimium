use minimium_astronomy::SkyState;
use minimium_core::universe::{
    BodyKind, CelestialBodyDefinition, CelestialBodyId, Ephemeris, LightEmitter, SolarSystemId,
    SurfaceOptics, UniverseCatalog,
};
use std::sync::OnceLock;

const DEMO_SYSTEM: SolarSystemId = SolarSystemId::new(50).expect("demo system ID is nonzero");
const DEMO_STAR: CelestialBodyId = CelestialBodyId::new(1001).expect("demo body ID is nonzero");
const DEMO_PLANET: CelestialBodyId = CelestialBodyId::new(1002).expect("demo body ID is nonzero");
const DEMO_MOON: CelestialBodyId = CelestialBodyId::new(1003).expect("demo body ID is nonzero");

pub(super) struct DemoCatalogue {
    pub(super) catalog: UniverseCatalog,
    pub(super) system: SolarSystemId,
}

pub(super) fn demo_catalogue() -> &'static DemoCatalogue {
    static DEMO: OnceLock<DemoCatalogue> = OnceLock::new();
    DEMO.get_or_init(|| build_demo_catalogue().expect("built-in demo catalog is valid"))
}

fn build_demo_catalogue() -> Result<DemoCatalogue, minimium_core::universe::CatalogError> {
    let mut catalog = UniverseCatalog::default();
    catalog.add_system(DEMO_SYSTEM)?;
    catalog.register(CelestialBodyDefinition {
        id: DEMO_STAR,
        system: DEMO_SYSTEM,
        kind: BodyKind::Star,
        parent: None,
        orbit: Ephemeris::Fixed([22_000_000.0, 0.0, 0.0]),
        radius_m: 1_400_000.0,
        emission: Some(LightEmitter {
            linear_rgb: [8.0, 5.5, 3.5],
            reference_distance_m: 10_000_000.0,
        }),
        optics: SurfaceOptics {
            albedo: 0.0,
            atmosphere_scattering: 0.0,
        },
        explorable: None,
    })?;
    catalog.register(CelestialBodyDefinition {
        id: DEMO_PLANET,
        system: DEMO_SYSTEM,
        kind: BodyKind::Planet,
        parent: Some(DEMO_STAR),
        orbit: Ephemeris::Circular {
            radius_m: 11_000_000.0,
            period_seconds: 86_400.0,
            phase_rad: 0.4,
        },
        radius_m: 1_000_000.0,
        emission: None,
        optics: SurfaceOptics {
            albedo: 0.5,
            atmosphere_scattering: 0.08,
        },
        explorable: None,
    })?;
    catalog.register(CelestialBodyDefinition {
        id: DEMO_MOON,
        system: DEMO_SYSTEM,
        kind: BodyKind::Moon,
        parent: Some(DEMO_PLANET),
        orbit: Ephemeris::Circular {
            radius_m: 3_000_000.0,
            period_seconds: 10_000.0,
            phase_rad: 1.1,
        },
        radius_m: 350_000.0,
        emission: None,
        optics: SurfaceOptics {
            albedo: 0.8,
            atmosphere_scattering: 0.15,
        },
        explorable: None,
    })?;
    Ok(DemoCatalogue {
        catalog,
        system: DEMO_SYSTEM,
    })
}

pub(super) fn observer_epoch_position(sky: &SkyState, observer_world_m: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|column| {
        (0..3)
            .map(|row| sky.catalog_epoch_to_world[row][column] * observer_world_m[row])
            .sum()
    })
}
