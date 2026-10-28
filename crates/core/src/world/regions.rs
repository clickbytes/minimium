//! Planet/dungeon topology in a single authoritative map.
//! Future independently instanced dungeons will require explicit realm IDs on the wire.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldKind {
    PlanetSurface,
    Dungeon,
}

/// A procedural planet with radial gravity. All positions are in world coordinates.
#[derive(Clone, Copy, Debug)]
pub struct CubeSphere {
    pub center: [f32; 3],
    pub radius: f32,
}

impl CubeSphere {
    pub fn up_at(self, position: [f32; 3]) -> [f32; 3] {
        let delta = [
            position[0] - self.center[0],
            position[1] - self.center[1],
            position[2] - self.center[2],
        ];
        let length = delta.iter().map(|x| x * x).sum::<f32>().sqrt();
        if length < 0.0001 || !length.is_finite() {
            [0.0, 1.0, 0.0]
        } else {
            delta.map(|value| value / length)
        }
    }

    /// Stable local east/up/north frame: at the north pole it is world X/Y/Z.
    pub fn frame_at(self, position: [f32; 3]) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let up = self.up_at(position);
        let reference = if up[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let dot = up.iter().zip(reference).map(|(a, b)| a * b).sum::<f32>();
        let mut east = [
            reference[0] - up[0] * dot,
            reference[1] - up[1] * dot,
            reference[2] - up[2] * dot,
        ];
        let length = east.iter().map(|x| x * x).sum::<f32>().sqrt();
        east = east.map(|value| value / length);
        let north = [
            east[1] * up[2] - east[2] * up[1],
            east[2] * up[0] - east[0] * up[2],
            east[0] * up[1] - east[1] * up[0],
        ];
        (east, up, north)
    }

    /// The dungeon is placed outside this local planet space in the MVP map.
    pub fn contains(self, position: [f32; 3]) -> bool {
        let limit = self.radius + 32.0;
        position
            .iter()
            .zip(self.center)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>()
            <= limit * limit
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WorldPortal {
    /// Feet position of the trigger, in its source world region.
    pub at: [f32; 3],
    /// Safe feet position in the destination world region.
    pub destination: [f32; 3],
    pub radius: f32,
}

pub fn portal_destination(position: [f32; 3], portals: &[WorldPortal]) -> Option<[f32; 3]> {
    portals.iter().find_map(|portal| {
        let distance_squared = position
            .iter()
            .zip(portal.at)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>();
        (distance_squared <= portal.radius * portal.radius).then_some(portal.destination)
    })
}
