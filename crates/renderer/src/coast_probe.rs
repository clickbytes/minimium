//! Cached presentation classification on the same lattice as dense render blocks.
//! Five corner/centre samples alone can miss whole islands or narrow channels.
use minimium_procgen::PlanetTerrain;
use minimium_world::world::{PlanetFace, PLANET_SAMPLE_COORDINATE_SCALE};
use std::collections::HashMap;

const MAX_PROBES: usize = 2048;
const STEPS: i64 = minimium_procgen::PLANET_RENDER_BLOCK_SUBDIVISIONS as i64;
type Key = (PlanetFace, u16, u16, u16);

#[derive(Clone, Copy)]
pub(crate) enum SurfaceClass {
    Land,
    Ocean,
    Coast,
}

#[derive(Default)]
pub(crate) struct CoastProbeCache {
    entries: HashMap<Key, (u64, SurfaceClass)>,
    tick: u64,
}

impl CoastProbeCache {
    pub fn classify(&mut self, terrain: &PlanetTerrain, key: Key, count: u16) -> SurfaceClass {
        self.tick = self.tick.wrapping_add(1);
        if let Some((age, class)) = self.entries.get_mut(&key) {
            *age = self.tick;
            return *class;
        }
        let class = classify(terrain, key, count);
        if self.entries.len() >= MAX_PROBES {
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, v)| v.0)
                .map(|(k, _)| *k)
            {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(key, (self.tick, class));
        class
    }
}

fn classify(terrain: &PlanetTerrain, (face, u, v, span): Key, count: u16) -> SurfaceClass {
    let scale = i64::from(PLANET_SAMPLE_COORDINATE_SCALE);
    let coordinate = |start: u16, step: i64| {
        (-scale
            + 2 * scale * (i64::from(start) * STEPS + i64::from(span) * step)
                / (i64::from(count) * STEPS)) as i32
    };
    let mut land = false;
    let mut ocean = false;
    for y in 0..=STEPS {
        for x in 0..=STEPS {
            let key = face
                .cube_sample(coordinate(u, x), coordinate(v, y))
                .expect("valid render block lattice");
            // Coast identity is independent of peak exaggeration and tiny relief.
            // Direct source sampling avoids full material/normal/noise evaluation.
            let water = terrain
                .source_elevation_m(key.coordinates().map(f64::from))
                .expect("Earth coast probes require prepared geography")
                < 0.0;
            ocean |= water;
            land |= !water;
            if land && ocean {
                return SurfaceClass::Coast;
            }
        }
    }
    if land {
        SurfaceClass::Land
    } else {
        SurfaceClass::Ocean
    }
}
