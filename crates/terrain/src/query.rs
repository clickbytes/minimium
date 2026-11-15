use crate::TerrainView;
use minimium_world::terrain::{
    valid_position, SurfaceHit, SurfaceTarget, TerrainChunkKey, TerrainRead, TerrainRevision,
    TerrainSample,
};

impl TerrainRead for TerrainView<'_> {
    fn sample(&self, position: [f64; 3]) -> Option<TerrainSample> {
        self.interpolated(position)
    }

    fn surface_hit(&self, origin: [f64; 3], direction: [f64; 3], reach: f64) -> Option<SurfaceHit> {
        let length = direction.iter().map(|v| v * v).sum::<f64>().sqrt();
        if !valid_position(origin)
            || !length.is_finite()
            || length < 1.0e-9
            || !(0.0..=256.0).contains(&reach)
        {
            return None;
        }
        let direction = direction.map(|v| v / length);
        let point = |t: f64| std::array::from_fn(|i| origin[i] + direction[i] * t);
        let initial = self.interpolated(origin)?.distance_m;
        let mut start = 0.0;
        let mut before = initial;
        while start < reach {
            let end = (start + 0.0625).min(reach);
            let after = self.interpolated(point(end))?.distance_m;
            if (before > 0.0) != (after > 0.0) || before == 0.0 {
                let mut lo = start;
                let mut hi = end;
                for _ in 0..24 {
                    let mid = (lo + hi) * 0.5;
                    if (self.interpolated(point(mid))?.distance_m > 0.0) == (before > 0.0) {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let distance_m = (lo + hi) * 0.5;
                let position_m = point(distance_m);
                let sample = self.interpolated(position_m)?;
                let key = TerrainChunkKey::at(self.store.world, position_m)?;
                return Some(SurfaceHit {
                    position_m,
                    normal: self.normal(position_m)?,
                    distance_m,
                    target: SurfaceTarget::Terrain(key),
                    material: sample.material,
                    revision: self
                        .store
                        .chunk(key)
                        .map_or(TerrainRevision(0), |c| c.revision()),
                });
            }
            start = end;
            before = after;
        }
        None
    }
}
