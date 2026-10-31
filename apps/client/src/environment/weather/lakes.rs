//! Offline, spatially indexed Natural Earth polygons. Rings retain island holes.
use std::sync::OnceLock;

struct Polygon {
    bounds: [f64; 4],
    rings: Vec<Vec<[f64; 2]>>,
}

struct Lakes {
    polygons: Vec<Polygon>,
    cells: Vec<Vec<usize>>,
}

fn cell(lat: f64, lon: f64) -> usize {
    let x = ((lon + 180.0).rem_euclid(360.0).floor() as usize).min(359);
    let y = ((lat + 90.0).floor() as usize).min(179);
    y * 360 + x
}

impl Lakes {
    fn load() -> Self {
        let bytes = include_bytes!("../../../../../assets/weather/lakes.bin");
        assert_eq!(&bytes[..8], b"MMLAKE01");
        let mut cursor = 8;
        let mut read = || {
            let value = i32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
            cursor += 4;
            value
        };
        let mut polygons = Vec::new();
        let mut cells = vec![Vec::new(); 360 * 180];
        for _ in 0..read() {
            let mut rings = Vec::new();
            let mut bounds = [180.0_f64, 90.0_f64, -180.0_f64, -90.0_f64];
            for _ in 0..read() {
                let mut ring = Vec::new();
                for _ in 0..read() {
                    let p = [f64::from(read()) / 100000.0, f64::from(read()) / 100000.0];
                    bounds[0] = bounds[0].min(p[0]);
                    bounds[1] = bounds[1].min(p[1]);
                    bounds[2] = bounds[2].max(p[0]);
                    bounds[3] = bounds[3].max(p[1]);
                    ring.push(p);
                }
                rings.push(ring);
            }
            let id = polygons.len();
            for lat in bounds[1].floor() as i32..=bounds[3].floor() as i32 {
                for lon in bounds[0].floor() as i32..=bounds[2].floor() as i32 {
                    cells[cell(f64::from(lat), f64::from(lon))].push(id);
                }
            }
            polygons.push(Polygon { bounds, rings });
        }
        Self { polygons, cells }
    }
}

/// Degrees, geodetic coordinates. Does not infer lakes from below-sea-level terrain.
pub(crate) fn contains(latitude: f64, longitude: f64) -> bool {
    static LAKES: OnceLock<Lakes> = OnceLock::new();
    let lakes = LAKES.get_or_init(Lakes::load);
    let lon = (longitude + 180.0).rem_euclid(360.0) - 180.0;
    lakes.cells[cell(latitude, lon)].iter().any(|id| {
        let polygon = &lakes.polygons[*id];
        let [west, south, east, north] = polygon.bounds;
        if lon < west || lon > east || latitude < south || latitude > north {
            return false;
        }
        let mut inside = false;
        for ring in &polygon.rings {
            for edge in ring.windows(2) {
                let [a, b] = [edge[0], edge[1]];
                if (a[1] > latitude) != (b[1] > latitude)
                    && lon < (b[0] - a[0]) * (latitude - a[1]) / (b[1] - a[1]) + a[0]
                {
                    inside = !inside;
                }
            }
        }
        inside
    })
}
