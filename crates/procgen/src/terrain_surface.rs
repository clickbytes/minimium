//! Presentation sampling; does not alter generated geometry or fingerprints.
use crate::PlanetTerrain;
use minimium_world::world::{CubeSampleKey, PLANET_SAMPLE_COORDINATE_SCALE};

/// Sand, soil, grass, litter, rock, snow. These are fractions, never RGB labels.
#[derive(Clone, Copy, Debug, Default)]
pub struct TerrainSurfaceSample {
    pub elevation_m: f32,
    /// Physical source metres, independently of the displayed planet radius.
    pub source_elevation_m: f32,
    pub landform: crate::surface_habitat::Landform,
    pub mapped_coverage: bool,
    pub slope: f32,
    pub water: f32,
    pub normal: [f32; 3],
    pub coverage: [f32; 6],
}

impl PlanetTerrain {
    pub fn surface_sample(&self, direction: [f64; 3]) -> TerrainSurfaceSample {
        let n = normalize(direction);
        let east = normalize(if n[1].abs() < 0.9 {
            cross([0.0, 1.0, 0.0], n)
        } else {
            cross([1.0, 0.0, 0.0], n)
        });
        let north = cross(n, east);
        // Constant world-space derivative radius shared by every sector and LOD.
        let epsilon =
            (2000.0 / self.radius_mm() as f64).max(4.0 / f64::from(PLANET_SAMPLE_COORDINATE_SCALE));
        let point = self.ground_sample(key(n));
        let height = |d| self.ground_sample(key(d)).height_mm as f64 / 1000.0;
        let derivative = |axis: [f64; 3]| {
            let plus = normalize(std::array::from_fn(|i| n[i] + axis[i] * epsilon));
            let minus = normalize(std::array::from_fn(|i| n[i] - axis[i] * epsilon));
            (height(plus) - height(minus)) / (2.0 * epsilon * self.radius_mm() as f64 / 1000.0)
        };
        let de = derivative(east);
        let dn = derivative(north);
        let normal = normalize(std::array::from_fn(|i| n[i] - east[i] * de - north[i] * dn));
        let slope = (1.0 - dot(normal, n)).clamp(0.0, 1.0) as f32;
        let elevation_m = (point.height_mm as i64 - self.radius_mm() as i64 - self.sea_level_mm())
            as f32
            / 1000.0;
        let radius_m = self.radius_mm() as f64 / 1000.0;
        let angle = (self.surface_recipe.landform_radius_m / radius_m).min(0.05);
        let offset = |axis: [f64; 3], sign: f64| {
            height(normalize(std::array::from_fn(|i| {
                n[i] + axis[i] * angle * sign
            })))
        };
        let landform = crate::surface_habitat::Landform::from_heights(
            point.height_mm as f64 / 1000.0,
            [
                offset(east, 1.0),
                offset(east, -1.0),
                offset(north, 1.0),
                offset(north, -1.0),
            ],
            slope,
            self.surface_recipe,
        );
        let sand = (-elevation_m.max(0.0) / 8.0).exp() * (1.0 - slope);
        let rock = ((slope - 0.08) / 0.35).clamp(0.0, 1.0);
        // Broad fallback when WorldCover is absent: arid belts and alpine terrain
        // must not inherit the same grass fraction as humid lowlands.
        let latitude = n[1].asin().abs() as f32;
        let dry_belt = (-((latitude - 0.5) / 0.18).powi(2)).exp();
        let physical_height = if self.has_peak_relief() {
            self.source_elevation_m(n)
                .expect("prepared Earth source elevation") as f32
        } else {
            elevation_m / self.presentation_height_scale()
        };
        let temperature = minimium_environment::climate::annual_temperature_c(n[1].asin()) as f32
            - physical_height.max(0.0) * 0.0065;
        let vegetation = ((temperature + 5.0) / 24.0).clamp(0.0, 1.0) * (1.0 - dry_belt * 0.9);
        let mut coverage = [
            sand + dry_belt * 0.65,
            0.22 + (1.0 - vegetation) * 0.45,
            vegetation * 0.65,
            vegetation * 0.25,
            rock * 3.0 + ((physical_height - 1800.0) / 1800.0).clamp(0.0, 1.0) * 0.5,
            0.0,
        ];
        let mapped = self.land_cover(n);
        if let Some(base) = mapped {
            coverage = base;
            for weight in &mut coverage {
                *weight *= 1.0 - rock;
            }
            coverage[4] += rock;
            coverage[0] += sand * 0.2;
        }
        // Loose material is retained in hollows; exposed convex ground shows rock.
        let exposed = landform.exposure * 0.35;
        let loose = coverage[0] + coverage[1];
        let deposit = coverage[0] * landform.retention * 0.25;
        coverage[0] -= deposit;
        coverage[1] += deposit;
        for weight in &mut coverage[..4] {
            *weight *= 1.0 - exposed;
        }
        coverage[4] += loose * exposed;
        let total: f32 = coverage.iter().sum();
        for weight in &mut coverage {
            *weight /= total;
        }
        TerrainSurfaceSample {
            elevation_m,
            source_elevation_m: physical_height,
            landform,
            mapped_coverage: mapped.is_some(),
            slope,
            water: f32::from(point.ocean),
            normal: normal.map(|v| v as f32),
            coverage,
        }
    }
}

pub fn key(direction: [f64; 3]) -> CubeSampleKey {
    let maximum = direction.iter().map(|v| v.abs()).fold(0.0_f64, f64::max);
    let coordinates =
        direction.map(|v| (v / maximum * f64::from(PLANET_SAMPLE_COORDINATE_SCALE)).round() as i32);
    use minimium_world::world::PlanetFace;
    let [x, y, z] = coordinates;
    let scale = PLANET_SAMPLE_COORDINATE_SCALE;
    let (face, u, v) = if x.abs() == scale {
        (
            if x > 0 {
                PlanetFace::PositiveX
            } else {
                PlanetFace::NegativeX
            },
            y,
            z,
        )
    } else if y.abs() == scale {
        (
            if y > 0 {
                PlanetFace::PositiveY
            } else {
                PlanetFace::NegativeY
            },
            x,
            z,
        )
    } else {
        (
            if z > 0 {
                PlanetFace::PositiveZ
            } else {
                PlanetFace::NegativeZ
            },
            x,
            y,
        )
    };
    face.cube_sample(u, v).expect("normalized cube surface")
}

fn normalize(p: [f64; 3]) -> [f64; 3] {
    let length = dot(p, p).sqrt().max(1e-20);
    p.map(|v| v / length)
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
