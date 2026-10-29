//! Version-one logical address spaces for planet and dungeon sectors.

pub const PLANET_SECTORS_PER_FACE: u16 = 256;
pub const PLANET_COLLISION_CELLS_PER_FACE: u16 = 4096;
pub const MAX_RENDER_LOD: u8 = 15;
pub const PLANET_SAMPLE_COORDINATE_SCALE: i32 =
    PLANET_SECTORS_PER_FACE as i32 * (1_i32 << MAX_RENDER_LOD);
const FACE_SECTOR_COORDINATE_SCALE: i32 = PLANET_SECTORS_PER_FACE as i32;
const SECTOR_SAMPLE_SPAN: u32 =
    (PLANET_SAMPLE_COORDINATE_SCALE as u32 * 2) / PLANET_SECTORS_PER_FACE as u32;

/// Face order is part of the v1 planet projection contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum PlanetFace {
    PositiveX = 0,
    NegativeX = 1,
    PositiveY = 2,
    NegativeY = 3,
    PositiveZ = 4,
    NegativeZ = 5,
}

impl PlanetFace {
    pub const ALL: [Self; 6] = [
        Self::PositiveX,
        Self::NegativeX,
        Self::PositiveY,
        Self::NegativeY,
        Self::PositiveZ,
        Self::NegativeZ,
    ];

    pub const fn index(self) -> u8 {
        self as u8
    }

    /// Cube projection basis frozen from the v1 face layout.
    const fn basis(self) -> ([i8; 3], [i8; 3], [i8; 3]) {
        match self {
            Self::PositiveX => ([1, 0, 0], [0, 1, 0], [0, 0, 1]),
            Self::NegativeX => ([-1, 0, 0], [0, 1, 0], [0, 0, 1]),
            Self::PositiveY => ([0, 1, 0], [1, 0, 0], [0, 0, 1]),
            Self::NegativeY => ([0, -1, 0], [1, 0, 0], [0, 0, 1]),
            Self::PositiveZ => ([0, 0, 1], [1, 0, 0], [0, 1, 0]),
            Self::NegativeZ => ([0, 0, -1], [1, 0, 0], [0, 1, 0]),
        }
    }

    /// Returns one canonical integer cube point for a face-local sample.
    /// Shared edge and corner samples produce identical coordinates on every face.
    pub fn cube_sample(self, u: i32, v: i32) -> Option<CubeSampleKey> {
        if !(-PLANET_SAMPLE_COORDINATE_SCALE..=PLANET_SAMPLE_COORDINATE_SCALE).contains(&u)
            || !(-PLANET_SAMPLE_COORDINATE_SCALE..=PLANET_SAMPLE_COORDINATE_SCALE).contains(&v)
        {
            return None;
        }
        let (normal, u_axis, v_axis) = self.basis();
        Some(CubeSampleKey([
            i32::from(normal[0]) * PLANET_SAMPLE_COORDINATE_SCALE
                + i32::from(u_axis[0]) * u
                + i32::from(v_axis[0]) * v,
            i32::from(normal[1]) * PLANET_SAMPLE_COORDINATE_SCALE
                + i32::from(u_axis[1]) * u
                + i32::from(v_axis[1]) * v,
            i32::from(normal[2]) * PLANET_SAMPLE_COORDINATE_SCALE
                + i32::from(u_axis[2]) * u
                + i32::from(v_axis[2]) * v,
        ]))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CubeSampleKey([i32; 3]);

impl CubeSampleKey {
    pub const fn coordinates(self) -> [i32; 3] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PlanetEdge {
    UNegative,
    UPositive,
    VNegative,
    VPositive,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlanetSectorKey {
    face: PlanetFace,
    u: u16,
    v: u16,
}

impl PlanetSectorKey {
    pub const fn new(face: PlanetFace, u: u16, v: u16) -> Option<Self> {
        if u < PLANET_SECTORS_PER_FACE && v < PLANET_SECTORS_PER_FACE {
            Some(Self { face, u, v })
        } else {
            None
        }
    }

    pub const fn face(self) -> PlanetFace {
        self.face
    }

    pub const fn coordinates(self) -> (u16, u16) {
        (self.u, self.v)
    }

    /// Maps a direction to the v1 face lattice using the dominant cube axis.
    pub fn from_direction(direction: [f32; 3]) -> Self {
        let absolute = direction.map(f32::abs);
        let face = if absolute[0] >= absolute[1] && absolute[0] >= absolute[2] {
            if direction[0] >= 0.0 {
                PlanetFace::PositiveX
            } else {
                PlanetFace::NegativeX
            }
        } else if absolute[1] >= absolute[2] {
            if direction[1] >= 0.0 {
                PlanetFace::PositiveY
            } else {
                PlanetFace::NegativeY
            }
        } else if direction[2] >= 0.0 {
            PlanetFace::PositiveZ
        } else {
            PlanetFace::NegativeZ
        };
        let (normal, u_axis, v_axis) = face.basis();
        let component = |axis: [i8; 3]| {
            direction[0] * f32::from(axis[0])
                + direction[1] * f32::from(axis[1])
                + direction[2] * f32::from(axis[2])
        };
        let denominator = component(normal).abs().max(f32::EPSILON);
        let coordinate = |axis: [i8; 3]| {
            let scaled = (component(axis) / denominator).clamp(-1.0, 1.0)
                * f32::from(PLANET_SECTORS_PER_FACE);
            (((scaled + f32::from(PLANET_SECTORS_PER_FACE))
                / (2.0 * f32::from(PLANET_SECTORS_PER_FACE)))
                * f32::from(PLANET_SECTORS_PER_FACE))
            .floor()
            .clamp(0.0, f32::from(PLANET_SECTORS_PER_FACE - 1)) as u16
        };
        Self {
            face,
            u: coordinate(u_axis),
            v: coordinate(v_axis),
        }
    }

    /// Resolves a face-local lattice point. Matching sector borders return the
    /// same cube key from either adjacent sector.
    pub fn cube_sample(
        self,
        local_u: u32,
        local_v: u32,
        subdivisions: u32,
    ) -> Option<CubeSampleKey> {
        if subdivisions == 0
            || subdivisions > (1_u32 << MAX_RENDER_LOD)
            || !SECTOR_SAMPLE_SPAN.is_multiple_of(subdivisions)
            || local_u > subdivisions
            || local_v > subdivisions
        {
            return None;
        }
        let step = SECTOR_SAMPLE_SPAN / subdivisions;
        let u = -(PLANET_SAMPLE_COORDINATE_SCALE)
            + i32::from(self.u) * SECTOR_SAMPLE_SPAN as i32
            + (local_u * step) as i32;
        let v = -(PLANET_SAMPLE_COORDINATE_SCALE)
            + i32::from(self.v) * SECTOR_SAMPLE_SPAN as i32
            + (local_v * step) as i32;
        self.face.cube_sample(u, v)
    }

    pub fn neighbor(self, edge: PlanetEdge) -> Self {
        let neighbor_u = match edge {
            PlanetEdge::UNegative if self.u > 0 => Some(self.u - 1),
            PlanetEdge::UPositive if self.u + 1 < PLANET_SECTORS_PER_FACE => Some(self.u + 1),
            _ => None,
        };
        if let Some(u) = neighbor_u {
            return Self { u, ..self };
        }
        let neighbor_v = match edge {
            PlanetEdge::VNegative if self.v > 0 => Some(self.v - 1),
            PlanetEdge::VPositive if self.v + 1 < PLANET_SECTORS_PER_FACE => Some(self.v + 1),
            _ => None,
        };
        if let Some(v) = neighbor_v {
            return Self { v, ..self };
        }
        neighbor_across_face(self, edge)
    }

    pub fn neighbors(self) -> [Self; 4] {
        [
            self.neighbor(PlanetEdge::UNegative),
            self.neighbor(PlanetEdge::UPositive),
            self.neighbor(PlanetEdge::VNegative),
            self.neighbor(PlanetEdge::VPositive),
        ]
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlanetCorner {
    signs: [i8; 3],
}

impl PlanetCorner {
    pub const fn new(x: i8, y: i8, z: i8) -> Option<Self> {
        if (x == -1 || x == 1) && (y == -1 || y == 1) && (z == -1 || z == 1) {
            Some(Self { signs: [x, y, z] })
        } else {
            None
        }
    }

    pub const fn signs(self) -> [i8; 3] {
        self.signs
    }

    pub fn touching_sectors(self) -> [PlanetSectorKey; 3] {
        let cube = self
            .signs
            .map(|sign| i32::from(sign) * FACE_SECTOR_COORDINATE_SCALE);
        let faces = [
            if self.signs[0] > 0 {
                PlanetFace::PositiveX
            } else {
                PlanetFace::NegativeX
            },
            if self.signs[1] > 0 {
                PlanetFace::PositiveY
            } else {
                PlanetFace::NegativeY
            },
            if self.signs[2] > 0 {
                PlanetFace::PositiveZ
            } else {
                PlanetFace::NegativeZ
            },
        ];
        faces.map(|face| {
            let (_, u_axis, v_axis) = face.basis();
            let u = sector_index(inward_center(dot(cube, u_axis)));
            let v = sector_index(inward_center(dot(cube, v_axis)));
            PlanetSectorKey { face, u, v }
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DungeonSectorKey {
    x: i32,
    y: i32,
    z: i32,
}

impl DungeonSectorKey {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub const fn coordinates(self) -> (i32, i32, i32) {
        (self.x, self.y, self.z)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SectorKey {
    Planet(PlanetSectorKey),
    Dungeon(DungeonSectorKey),
}

impl SectorKey {
    pub const fn planet(face: PlanetFace, u: u16, v: u16) -> Option<Self> {
        match PlanetSectorKey::new(face, u, v) {
            Some(key) => Some(Self::Planet(key)),
            None => None,
        }
    }

    pub const fn dungeon(x: i32, y: i32, z: i32) -> Self {
        Self::Dungeon(DungeonSectorKey::new(x, y, z))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlanetCollisionCell {
    face: PlanetFace,
    u: u16,
    v: u16,
}

impl PlanetCollisionCell {
    pub const fn new(face: PlanetFace, u: u16, v: u16) -> Option<Self> {
        if u < PLANET_COLLISION_CELLS_PER_FACE && v < PLANET_COLLISION_CELLS_PER_FACE {
            Some(Self { face, u, v })
        } else {
            None
        }
    }

    pub const fn face(self) -> PlanetFace {
        self.face
    }

    pub const fn coordinates(self) -> (u16, u16) {
        (self.u, self.v)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DungeonCollisionCell {
    x: i32,
    y: i32,
    z: i32,
}

impl DungeonCollisionCell {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub const fn coordinates(self) -> (i32, i32, i32) {
        (self.x, self.y, self.z)
    }
}

/// Collision addressing is deliberately independent from sector streaming keys.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CollisionCell {
    Planet(PlanetCollisionCell),
    Dungeon(DungeonCollisionCell),
}

/// Renderer subdivision address. It never changes persistent sector identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RenderPatchKey {
    sector: SectorKey,
    lod: u8,
    x: u16,
    y: u16,
}

impl RenderPatchKey {
    pub const fn new(sector: SectorKey, lod: u8, x: u16, y: u16) -> Option<Self> {
        if lod > MAX_RENDER_LOD {
            return None;
        }
        let patches = 1_u32 << lod;
        if (x as u32) < patches && (y as u32) < patches {
            Some(Self { sector, lod, x, y })
        } else {
            None
        }
    }

    pub const fn sector(self) -> SectorKey {
        self.sector
    }

    pub const fn lod(self) -> u8 {
        self.lod
    }

    pub const fn coordinates(self) -> (u16, u16) {
        (self.x, self.y)
    }
}

fn neighbor_across_face(key: PlanetSectorKey, edge: PlanetEdge) -> PlanetSectorKey {
    let (normal, u_axis, v_axis) = key.face.basis();
    let u_center = sector_center(key.u);
    let v_center = sector_center(key.v);
    let (adjacent_normal, u, v) = match edge {
        PlanetEdge::UNegative => (negate(u_axis), -FACE_SECTOR_COORDINATE_SCALE, v_center),
        PlanetEdge::UPositive => (u_axis, FACE_SECTOR_COORDINATE_SCALE, v_center),
        PlanetEdge::VNegative => (negate(v_axis), u_center, -FACE_SECTOR_COORDINATE_SCALE),
        PlanetEdge::VPositive => (v_axis, u_center, FACE_SECTOR_COORDINATE_SCALE),
    };
    let face = face_from_normal(adjacent_normal);
    let cube = [
        i32::from(normal[0]) * FACE_SECTOR_COORDINATE_SCALE
            + i32::from(u_axis[0]) * u
            + i32::from(v_axis[0]) * v,
        i32::from(normal[1]) * FACE_SECTOR_COORDINATE_SCALE
            + i32::from(u_axis[1]) * u
            + i32::from(v_axis[1]) * v,
        i32::from(normal[2]) * FACE_SECTOR_COORDINATE_SCALE
            + i32::from(u_axis[2]) * u
            + i32::from(v_axis[2]) * v,
    ];
    let (_, adjacent_u, adjacent_v) = face.basis();
    let mapped_u = sector_index(inward_center(dot(cube, adjacent_u)));
    let mapped_v = sector_index(inward_center(dot(cube, adjacent_v)));
    PlanetSectorKey {
        face,
        u: mapped_u,
        v: mapped_v,
    }
}

fn face_from_normal(normal: [i8; 3]) -> PlanetFace {
    match normal {
        [1, 0, 0] => PlanetFace::PositiveX,
        [-1, 0, 0] => PlanetFace::NegativeX,
        [0, 1, 0] => PlanetFace::PositiveY,
        [0, -1, 0] => PlanetFace::NegativeY,
        [0, 0, 1] => PlanetFace::PositiveZ,
        _ => PlanetFace::NegativeZ,
    }
}

fn sector_center(index: u16) -> i32 {
    i32::from(index) * 2 - (FACE_SECTOR_COORDINATE_SCALE - 1)
}

fn sector_index(center: i32) -> u16 {
    ((center + FACE_SECTOR_COORDINATE_SCALE - 1) / 2) as u16
}

fn inward_center(coordinate: i32) -> i32 {
    if coordinate == FACE_SECTOR_COORDINATE_SCALE {
        coordinate - 1
    } else if coordinate == -FACE_SECTOR_COORDINATE_SCALE {
        coordinate + 1
    } else {
        coordinate
    }
}

fn dot(coordinates: [i32; 3], axis: [i8; 3]) -> i32 {
    coordinates[0] * i32::from(axis[0])
        + coordinates[1] * i32::from(axis[1])
        + coordinates[2] * i32::from(axis[2])
}

fn negate(axis: [i8; 3]) -> [i8; 3] {
    axis.map(|value| -value)
}
