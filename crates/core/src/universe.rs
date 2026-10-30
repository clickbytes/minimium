//! Lightweight solar-system definition graph. It does not simulate galaxies or
//! N-body gravitation. A body may exist without an explorable WorldDefinitionId.
use crate::world::WorldDefinitionId;
use std::{collections::BTreeMap, f64::consts::PI};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SolarSystemId(u64);
impl SolarSystemId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CelestialBodyId(u64);
impl CelestialBodyId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyKind {
    Star,
    Planet,
    Moon,
    MinorBody,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ephemeris {
    /// Position relative to parent, in metres, within the system's inertial frame.
    Fixed([f64; 3]),
    /// Simplified circular orbit used until a precise ephemeris provider exists.
    Circular {
        radius_m: f64,
        period_seconds: f64,
        phase_rad: f64,
    },
}
impl Ephemeris {
    fn position_at(self, time_seconds: f64) -> [f64; 3] {
        match self {
            Self::Fixed(position) => position,
            Self::Circular {
                radius_m,
                period_seconds,
                phase_rad,
            } => {
                let angle = phase_rad + 2.0 * PI * (time_seconds / period_seconds);
                [radius_m * angle.cos(), radius_m * angle.sin(), 0.0]
            }
        }
    }
    fn is_valid(self) -> bool {
        match self {
            Self::Fixed(value) => value.iter().all(|v| v.is_finite()),
            Self::Circular {
                radius_m,
                period_seconds,
                phase_rad,
            } => {
                radius_m.is_finite()
                    && radius_m > 0.0
                    && period_seconds.is_finite()
                    && period_seconds > 0.0
                    && phase_rad.is_finite()
            }
        }
    }
}

/// Relative source RGB at the declared reference distance (not a SI radiometry solver).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightEmitter {
    pub linear_rgb: [f64; 3],
    pub reference_distance_m: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceOptics {
    pub albedo: f64,
    /// Dimensionless fraction of illuminated radiance scattered into a limb glow.
    pub atmosphere_scattering: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CelestialBodyDefinition {
    pub id: CelestialBodyId,
    pub system: SolarSystemId,
    pub kind: BodyKind,
    pub parent: Option<CelestialBodyId>,
    pub orbit: Ephemeris,
    pub radius_m: f64,
    pub emission: Option<LightEmitter>,
    pub optics: SurfaceOptics,
    pub explorable: Option<WorldDefinitionId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogError {
    UnknownSystem,
    DuplicateSystem,
    DuplicateBody,
    MissingParent,
    CrossSystemParent,
    InvalidBody,
    InvalidTime,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyObservation {
    pub id: CelestialBodyId,
    pub kind: BodyKind,
    pub direction: [f64; 3],
    pub distance_m: f64,
    pub angular_radius_rad: f64,
    pub emitted_linear_rgb: [f64; 3],
    pub reflected_linear_rgb: [f64; 3],
    pub atmosphere_limb_rgb: [f64; 3],
    pub illuminated_fraction: f64,
    pub visible: bool,
}

#[derive(Default)]
pub struct UniverseCatalog {
    systems: BTreeMap<SolarSystemId, Vec<CelestialBodyId>>,
    bodies: BTreeMap<CelestialBodyId, CelestialBodyDefinition>,
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn length(v: [f64; 3]) -> f64 {
    dot(v, v).sqrt()
}
fn normalize(v: [f64; 3]) -> [f64; 3] {
    let d = length(v).max(1.0e-12);
    v.map(|n| n / d)
}
fn scale_rgb(rgb: [f64; 3], factor: f64) -> [f64; 3] {
    rgb.map(|value| value * factor)
}

fn segment_intersects_sphere(a: [f64; 3], b: [f64; 3], center: [f64; 3], radius: f64) -> bool {
    let ab = sub(b, a);
    let denominator = dot(ab, ab);
    if denominator < 1.0e-18 {
        return false;
    }
    let t = (dot(sub(center, a), ab) / denominator).clamp(0.0, 1.0);
    let closest = [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t];
    dot(sub(closest, center), sub(closest, center)) < radius * radius
}

impl UniverseCatalog {
    pub fn add_system(&mut self, id: SolarSystemId) -> Result<(), CatalogError> {
        if self.systems.contains_key(&id) {
            return Err(CatalogError::DuplicateSystem);
        }
        self.systems.insert(id, Vec::new());
        Ok(())
    }

    pub fn register(&mut self, body: CelestialBodyDefinition) -> Result<(), CatalogError> {
        if self.bodies.contains_key(&body.id) {
            return Err(CatalogError::DuplicateBody);
        }
        if !self.systems.contains_key(&body.system) {
            return Err(CatalogError::UnknownSystem);
        }
        if let Some(parent) = body.parent {
            let parent_body = self
                .bodies
                .get(&parent)
                .ok_or(CatalogError::MissingParent)?;
            if parent_body.system != body.system {
                return Err(CatalogError::CrossSystemParent);
            }
        }
        if !body.radius_m.is_finite()
            || body.radius_m <= 0.0
            || !body.orbit.is_valid()
            || !body.optics.albedo.is_finite()
            || !(0.0..=1.0).contains(&body.optics.albedo)
            || !body.optics.atmosphere_scattering.is_finite()
            || !(0.0..=1.0).contains(&body.optics.atmosphere_scattering)
            || body.emission.is_some_and(|light| {
                !light.reference_distance_m.is_finite()
                    || light.reference_distance_m <= 0.0
                    || !light
                        .linear_rgb
                        .iter()
                        .all(|channel| channel.is_finite() && *channel >= 0.0)
            })
        {
            return Err(CatalogError::InvalidBody);
        }
        self.systems
            .get_mut(&body.system)
            .expect("system checked")
            .push(body.id);
        self.bodies.insert(body.id, body);
        Ok(())
    }

    pub fn body(&self, id: CelestialBodyId) -> Option<&CelestialBodyDefinition> {
        self.bodies.get(&id)
    }
    pub fn bodies_in(
        &self,
        system: SolarSystemId,
    ) -> impl Iterator<Item = &CelestialBodyDefinition> {
        self.systems
            .get(&system)
            .into_iter()
            .flatten()
            .filter_map(|id| self.body(*id))
    }

    pub fn position_at(
        &self,
        id: CelestialBodyId,
        time_seconds: f64,
    ) -> Result<[f64; 3], CatalogError> {
        if !time_seconds.is_finite() {
            return Err(CatalogError::InvalidTime);
        }
        let body = self.bodies.get(&id).ok_or(CatalogError::InvalidBody)?;
        let offset = body.orbit.position_at(time_seconds);
        if let Some(parent) = body.parent {
            let center = self.position_at(parent, time_seconds)?;
            Ok([
                center[0] + offset[0],
                center[1] + offset[1],
                center[2] + offset[2],
            ])
        } else {
            Ok(offset)
        }
    }

    /// Evaluate a bounded list of sources with phase, full occlusion and a
    /// simple atmosphere-scattering estimate. Rendering handles glare/bloom.
    pub fn observe(
        &self,
        system: SolarSystemId,
        observer_m: [f64; 3],
        time_seconds: f64,
    ) -> Result<Vec<BodyObservation>, CatalogError> {
        if !observer_m.iter().all(|v| v.is_finite()) || !time_seconds.is_finite() {
            return Err(CatalogError::InvalidTime);
        }
        if !self.systems.contains_key(&system) {
            return Err(CatalogError::UnknownSystem);
        }
        let definitions: Vec<_> = self.bodies_in(system).copied().collect();
        let positions = definitions
            .iter()
            .map(|body| self.position_at(body.id, time_seconds))
            .collect::<Result<Vec<_>, _>>()?;
        let mut observations = Vec::with_capacity(definitions.len());
        for (index, body) in definitions.iter().enumerate() {
            let target = positions[index];
            let to_body = sub(target, observer_m);
            let distance = length(to_body);
            let visible = !definitions.iter().enumerate().any(|(j, occluder)| {
                j != index
                    && length(sub(positions[j], observer_m)) < distance
                    && segment_intersects_sphere(
                        observer_m,
                        target,
                        positions[j],
                        occluder.radius_m,
                    )
            });
            let emitted = body.emission.map_or([0.0; 3], |light| {
                scale_rgb(
                    light.linear_rgb,
                    (light.reference_distance_m / distance.max(1.0)).powi(2),
                )
            });
            let mut reflected = [0.0; 3];
            let mut illuminated_sum = 0.0;
            let mut illumination_weight = 0.0;
            for (j, source) in definitions.iter().enumerate() {
                let Some(light) = source.emission else {
                    continue;
                };
                if j == index {
                    continue;
                }
                let from_target = sub(positions[j], target);
                let source_distance = length(from_target);
                let occluded = definitions.iter().enumerate().any(|(k, obstacle)| {
                    k != index
                        && k != j
                        && segment_intersects_sphere(
                            target,
                            positions[j],
                            positions[k],
                            obstacle.radius_m,
                        )
                });
                if occluded {
                    continue;
                }
                let cos_phase = dot(normalize(from_target), normalize(sub(observer_m, target)))
                    .clamp(-1.0, 1.0);
                let alpha = cos_phase.acos();
                let phase = ((alpha.sin() + (PI - alpha) * alpha.cos()) / PI).clamp(0.0, 1.0);
                let flux = (light.reference_distance_m / source_distance.max(1.0)).powi(2)
                    * body.optics.albedo
                    * phase;
                // An apparent-disk fraction keeps far-away reflectors dim.
                let disk_factor = (body.radius_m / distance.max(body.radius_m)).powi(2);
                let rgb = scale_rgb(light.linear_rgb, flux * disk_factor);
                for c in 0..3 {
                    reflected[c] += rgb[c];
                }
                // Geometric illuminated fraction is independent of albedo and phase brightness.
                let illumination = ((1.0 + cos_phase) * 0.5).clamp(0.0, 1.0);
                let weight = (light.reference_distance_m / source_distance.max(1.0)).powi(2)
                    * light.linear_rgb.iter().sum::<f64>();
                illuminated_sum += illumination * weight;
                illumination_weight += weight;
            }
            let reflected = if visible { reflected } else { [0.0; 3] };
            let emitted = if visible { emitted } else { [0.0; 3] };
            let atmosphere_limb_rgb = scale_rgb(reflected, body.optics.atmosphere_scattering);
            let illuminated_fraction = if illumination_weight > 0.0 {
                illuminated_sum / illumination_weight
            } else {
                0.0
            };
            observations.push(BodyObservation {
                id: body.id,
                kind: body.kind,
                direction: normalize(to_body),
                distance_m: distance,
                angular_radius_rad: (body.radius_m / distance.max(body.radius_m)).asin(),
                emitted_linear_rgb: emitted,
                reflected_linear_rgb: reflected,
                atmosphere_limb_rgb,
                illuminated_fraction,
                visible,
            });
        }
        Ok(observations)
    }
}
