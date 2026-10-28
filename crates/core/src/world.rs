use crate::executor::Executor;
use std::{
    collections::{HashMap, HashSet},
    fmt::Write as FmtWrite,
    fs::File,
    io::{self, Read},
    path::{Component, Path},
    sync::Arc,
};

const CELL_SIZE: f32 = 32.0;
const VISIBILITY_RADIUS: f32 = 72.0;
const PLAYER_RADIUS: f32 = 0.3;
const PLAYER_HEIGHT: f32 = 1.7;
const MAX_STEP_UP: f32 = 0.55;
const PLAYER_MOVE_SPEED: f32 = 5.0;
const PLAYER_JUMP_SPEED: f32 = 5.5;
pub const PLAYER_MAX_HEALTH: u16 = 100;
const HITSCAN_DAMAGE: u16 = 34;
const HITSCAN_RANGE: f32 = 60.0;
const MAX_RAY_CELL_CROSSINGS: usize = 32;
const GRAVITY: f32 = 9.81;
const MAX_FALL_SPEED: f32 = 40.0;
const COLLISION_EPSILON: f32 = 0.0001;
const MAX_COLLIDER_CELLS: u64 = 64;
const MAX_QUERY_CELLS: u64 = 512;
const PARALLEL_SIMULATION_MIN_PLAYERS: usize = 32;
const PLAYERS_PER_SIMULATION_BATCH: usize = 16;

#[derive(Clone, Debug)]
pub struct Player {
    pub id: u64,
    pub name: Arc<str>,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub grounded: bool,
    pub health: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShotOutcome {
    pub target_id: Option<u64>,
    pub remaining_health: Option<u16>,
    pub killed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CharacterInput {
    pub direction: [f32; 2],
    pub jump: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CharacterState {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub grounded: bool,
}

impl CharacterState {
    pub fn at_spawn(spawn: [f32; 3]) -> Self {
        Self {
            position: spawn,
            ..Self::default()
        }
    }
}

pub fn step_character(
    state: &mut CharacterState,
    input: CharacterInput,
    colliders: &[Aabb],
    surfaces: &[MapTriangle],
    delta_seconds: f32,
) {
    if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
        return;
    }

    let mut direction = [
        finite_input(input.direction[0]),
        finite_input(input.direction[1]),
    ];
    let direction_length_squared = direction[0] * direction[0] + direction[1] * direction[1];
    if direction_length_squared > 1.0 {
        let inverse_length = direction_length_squared.sqrt().recip();
        direction[0] *= inverse_length;
        direction[1] *= inverse_length;
    }
    state.velocity[0] = direction[0] * PLAYER_MOVE_SPEED;
    state.velocity[2] = direction[1] * PLAYER_MOVE_SPEED;

    let should_jump = input.jump && state.grounded;
    state.grounded = false;
    if should_jump {
        state.velocity[1] = PLAYER_JUMP_SPEED;
    }
    state.velocity[1] = (state.velocity[1] - GRAVITY * delta_seconds).max(-MAX_FALL_SPEED);

    let old_y = state.position[1];
    let mut next_y = old_y + state.velocity[1] * delta_seconds;
    if state.velocity[1] <= 0.0 {
        let mut landing_height = None::<f32>;
        for collider in colliders.iter().copied() {
            if horizontal_overlap(state.position, collider)
                && old_y >= collider.max[1] - COLLISION_EPSILON
                && next_y <= collider.max[1]
                && landing_height.is_none_or(|height| collider.max[1] > height)
            {
                landing_height = Some(collider.max[1]);
            }
        }
        for surface in surfaces {
            if let Some(height) = triangle_height_for_player(state.position, *surface) {
                if old_y >= height - COLLISION_EPSILON
                    && next_y <= height
                    && landing_height.is_none_or(|current| height > current)
                {
                    landing_height = Some(height);
                }
            }
        }
        if let Some(height) = landing_height {
            next_y = height;
            state.velocity[1] = 0.0;
            state.grounded = true;
        }
    } else {
        let mut ceiling_height = None::<f32>;
        for collider in colliders.iter().copied() {
            if horizontal_overlap(state.position, collider)
                && old_y + PLAYER_HEIGHT <= collider.min[1] + COLLISION_EPSILON
                && next_y + PLAYER_HEIGHT >= collider.min[1]
                && ceiling_height.is_none_or(|height| collider.min[1] < height)
            {
                ceiling_height = Some(collider.min[1]);
            }
        }
        for surface in surfaces {
            if let Some(height) = triangle_height_for_player(state.position, *surface) {
                if old_y + PLAYER_HEIGHT <= height + COLLISION_EPSILON
                    && next_y + PLAYER_HEIGHT >= height
                    && ceiling_height.is_none_or(|current| height < current)
                {
                    ceiling_height = Some(height);
                }
            }
        }
        if let Some(height) = ceiling_height {
            next_y = height - PLAYER_HEIGHT;
            state.velocity[1] = 0.0;
        }
    }
    if let Some(safe_y) = clip_vertical_motion_to_triangles(state.position, old_y, next_y, surfaces)
    {
        next_y = safe_y;
        state.velocity[1] = 0.0;
        state.grounded = false;
    }
    state.position[1] = next_y;

    for axis in [0usize, 2] {
        let change = state.velocity[axis] * delta_seconds;
        if change == 0.0 {
            continue;
        }
        let mut candidate = state.position[axis] + change;
        let mut proposed = state.position;
        proposed[axis] = candidate;
        let mut stepped_position = None;
        if state.grounded {
            for collider in colliders.iter().copied() {
                if !intersects_player(proposed, collider) {
                    continue;
                }
                let step_up = collider.max[1] - state.position[1];
                if step_up <= 0.0 || step_up > MAX_STEP_UP {
                    continue;
                }
                let mut stepped = proposed;
                stepped[1] = collider.max[1];
                if colliders
                    .iter()
                    .all(|other| !intersects_player(stepped, *other))
                    && surfaces.iter().copied().all(|surface| {
                        is_walkable_triangle(surface)
                            || !triangle_intersects_player(stepped, surface)
                    })
                {
                    stepped_position = Some(stepped);
                    break;
                }
            }
        }
        if let Some(stepped) = stepped_position {
            state.position = stepped;
            state.velocity[1] = 0.0;
            continue;
        }

        for collider in colliders.iter().copied() {
            proposed = state.position;
            proposed[axis] = candidate;
            if intersects_player(proposed, collider) {
                candidate = match (axis, change.is_sign_positive()) {
                    (0, true) => collider.min[0] - PLAYER_RADIUS,
                    (0, false) => collider.max[0] + PLAYER_RADIUS,
                    (2, true) => collider.min[2] - PLAYER_RADIUS,
                    (2, false) => collider.max[2] + PLAYER_RADIUS,
                    _ => candidate,
                };
            }
        }
        for surface in surfaces.iter().copied() {
            if !is_walkable_triangle(surface) {
                proposed = state.position;
                proposed[axis] = candidate;
                if triangle_intersects_player(proposed, surface) {
                    candidate = state.position[axis];
                    break;
                }
            }
        }
        state.position[axis] = candidate;
    }

    if state.grounded {
        if let Some(height) = highest_triangle_height_for_player(state.position, surfaces) {
            let step = height - state.position[1];
            if (-MAX_STEP_UP..=MAX_STEP_UP).contains(&step) {
                state.position[1] = height;
                state.velocity[1] = 0.0;
            }
        }
    }
}

fn finite_input(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Cell(i32, i32, i32);

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub fn from_center_size(center: [f32; 3], size: [f32; 3]) -> Self {
        let half = [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5];
        Self {
            min: [
                center[0] - half[0],
                center[1] - half[1],
                center[2] - half[2],
            ],
            max: [
                center[0] + half[0],
                center[1] + half[1],
                center[2] + half[2],
            ],
        }
    }
}

#[derive(Clone, Default)]
pub struct CollisionIndex {
    colliders: Vec<Aabb>,
    cells: HashMap<Cell, Vec<usize>>,
    large_colliders: Vec<usize>,
    surfaces: Vec<MapTriangle>,
    surface_cells: HashMap<Cell, Vec<usize>>,
    large_surfaces: Vec<usize>,
}

#[derive(Default)]
struct PlayerRayQuery {
    candidates: Vec<u64>,
    seen: HashSet<u64>,
}

impl PlayerRayQuery {
    fn fill(
        &mut self,
        cells: &HashMap<Cell, Vec<u64>>,
        origin: [f32; 3],
        direction: [f32; 3],
        maximum_distance: f32,
    ) {
        self.candidates.clear();
        self.seen.clear();
        let candidates = &mut self.candidates;
        let seen = &mut self.seen;
        visit_ray_cells(origin, direction, maximum_distance, |cell| {
            for z_offset in -1..=1 {
                for y_offset in -1..=1 {
                    for x_offset in -1..=1 {
                        let nearby = Cell(cell.0 + x_offset, cell.1 + y_offset, cell.2 + z_offset);
                        if let Some(ids) = cells.get(&nearby) {
                            for id in ids {
                                if seen.insert(*id) {
                                    candidates.push(*id);
                                }
                            }
                        }
                    }
                }
            }
        });
    }
}

#[derive(Default)]
struct MapRayQuery {
    collider_candidates: Vec<usize>,
    seen_colliders: Vec<u32>,
    surface_candidates: Vec<usize>,
    seen_surfaces: Vec<u32>,
    generation: u32,
}

impl MapRayQuery {
    fn fill(
        &mut self,
        index: &CollisionIndex,
        origin: [f32; 3],
        direction: [f32; 3],
        maximum_distance: f32,
    ) {
        self.collider_candidates.clear();
        self.surface_candidates.clear();
        let mut reset_generation = false;
        if self.seen_colliders.len() != index.colliders.len() {
            self.seen_colliders.resize(index.colliders.len(), 0);
            reset_generation = true;
        }
        if self.seen_surfaces.len() != index.surfaces.len() {
            self.seen_surfaces.resize(index.surfaces.len(), 0);
            reset_generation = true;
        }
        if reset_generation {
            self.seen_colliders.fill(0);
            self.seen_surfaces.fill(0);
            self.generation = 0;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen_colliders.fill(0);
            self.seen_surfaces.fill(0);
            self.generation = 1;
        }
        let generation = self.generation;
        for candidate in &index.large_colliders {
            self.mark_collider(*candidate, generation);
        }
        for candidate in &index.large_surfaces {
            self.mark_surface(*candidate, generation);
        }
        let (seen_colliders, collider_candidates) =
            (&mut self.seen_colliders, &mut self.collider_candidates);
        let (seen_surfaces, surface_candidates) =
            (&mut self.seen_surfaces, &mut self.surface_candidates);
        visit_ray_cells(origin, direction, maximum_distance, |cell| {
            if let Some(indices) = index.cells.get(&cell) {
                for candidate in indices {
                    let candidate = *candidate;
                    if seen_colliders[candidate] != generation {
                        seen_colliders[candidate] = generation;
                        collider_candidates.push(candidate);
                    }
                }
            }
            if let Some(indices) = index.surface_cells.get(&cell) {
                for candidate in indices {
                    let candidate = *candidate;
                    if seen_surfaces[candidate] != generation {
                        seen_surfaces[candidate] = generation;
                        surface_candidates.push(candidate);
                    }
                }
            }
        });
    }

    fn mark_collider(&mut self, candidate: usize, generation: u32) {
        if self.seen_colliders[candidate] != generation {
            self.seen_colliders[candidate] = generation;
            self.collider_candidates.push(candidate);
        }
    }

    fn mark_surface(&mut self, candidate: usize, generation: u32) {
        if self.seen_surfaces[candidate] != generation {
            self.seen_surfaces[candidate] = generation;
            self.surface_candidates.push(candidate);
        }
    }

    fn nearest_blocker(
        &self,
        index: &CollisionIndex,
        origin: [f32; 3],
        direction: [f32; 3],
        maximum_distance: f32,
    ) -> Option<f32> {
        let collider_distance = self
            .collider_candidates
            .iter()
            .filter_map(|candidate| {
                ray_aabb_distance(
                    origin,
                    direction,
                    index.colliders[*candidate],
                    maximum_distance,
                )
            })
            .min_by(f32::total_cmp);
        let surface_distance = self
            .surface_candidates
            .iter()
            .filter_map(|candidate| {
                ray_triangle_distance(
                    origin,
                    direction,
                    index.surfaces[*candidate],
                    maximum_distance,
                )
            })
            .min_by(f32::total_cmp);
        match (collider_distance, surface_distance) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (Some(distance), None) | (None, Some(distance)) => Some(distance),
            (None, None) => None,
        }
    }
}

impl CollisionIndex {
    pub fn from_colliders(colliders: Vec<Aabb>) -> Self {
        Self::from_geometry(colliders, Vec::new())
    }

    pub fn from_geometry(colliders: Vec<Aabb>, surfaces: Vec<MapTriangle>) -> Self {
        let mut index = Self::default();
        for collider in colliders {
            index.add(collider);
        }
        for surface in surfaces {
            if surface.solid {
                index.add_surface(surface);
            }
        }
        index
    }

    fn add(&mut self, collider: Aabb) {
        if collider
            .min
            .iter()
            .chain(collider.max.iter())
            .any(|value| !value.is_finite())
            || (0..3).any(|axis| collider.min[axis] > collider.max[axis])
        {
            return;
        }

        let index = self.colliders.len();
        self.colliders.push(collider);
        let min = cell_for(collider.min);
        let max = cell_for(collider.max);
        let cell_count = cell_volume(min, max);
        if cell_count > MAX_COLLIDER_CELLS {
            self.large_colliders.push(index);
            return;
        }

        for z in min.2..=max.2 {
            for y in min.1..=max.1 {
                for x in min.0..=max.0 {
                    self.cells.entry(Cell(x, y, z)).or_default().push(index);
                }
            }
        }
    }

    fn add_surface(&mut self, surface: MapTriangle) {
        if !is_valid_solid_triangle(surface) {
            return;
        }
        let mut min = surface.vertices[0];
        let mut max = surface.vertices[0];
        for vertex in surface.vertices.iter().skip(1) {
            for axis in 0..3 {
                min[axis] = min[axis].min(vertex[axis]);
                max[axis] = max[axis].max(vertex[axis]);
            }
        }
        let index = self.surfaces.len();
        self.surfaces.push(surface);
        let min_cell = cell_for(min);
        let max_cell = cell_for(max);
        if cell_volume(min_cell, max_cell) > MAX_COLLIDER_CELLS {
            self.large_surfaces.push(index);
            return;
        }
        for z in min_cell.2..=max_cell.2 {
            for y in min_cell.1..=max_cell.1 {
                for x in min_cell.0..=max_cell.0 {
                    self.surface_cells
                        .entry(Cell(x, y, z))
                        .or_default()
                        .push(index);
                }
            }
        }
    }
}

#[derive(Default)]
pub struct CollisionQuery {
    candidates: Vec<Aabb>,
    seen: Vec<u32>,
    surface_candidates: Vec<MapTriangle>,
    seen_surfaces: Vec<u32>,
    generation: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CollisionQueryStats {
    pub queries: u64,
    pub cell_visits: u64,
    pub aabb_candidates: u64,
    pub triangle_candidates: u64,
    pub fallback_queries: u64,
    pub max_cell_visits: u64,
    pub max_aabb_candidates: u64,
    pub max_triangle_candidates: u64,
}

impl CollisionQueryStats {
    fn accumulate(&mut self, other: Self) {
        self.queries = self.queries.saturating_add(other.queries);
        self.cell_visits = self.cell_visits.saturating_add(other.cell_visits);
        self.aabb_candidates = self.aabb_candidates.saturating_add(other.aabb_candidates);
        self.triangle_candidates = self
            .triangle_candidates
            .saturating_add(other.triangle_candidates);
        self.fallback_queries = self.fallback_queries.saturating_add(other.fallback_queries);
        self.max_cell_visits = self.max_cell_visits.max(other.max_cell_visits);
        self.max_aabb_candidates = self.max_aabb_candidates.max(other.max_aabb_candidates);
        self.max_triangle_candidates = self
            .max_triangle_candidates
            .max(other.max_triangle_candidates);
    }
}

impl CollisionQuery {
    pub fn fill(
        &mut self,
        index: &CollisionIndex,
        state: CharacterState,
        delta_seconds: f32,
    ) -> CollisionQueryStats {
        let mut stats = CollisionQueryStats {
            queries: 1,
            ..CollisionQueryStats::default()
        };
        self.candidates.clear();
        self.surface_candidates.clear();
        let mut reset_generation = false;
        if self.seen.len() != index.colliders.len() {
            self.seen.resize(index.colliders.len(), 0);
            reset_generation = true;
        }
        if self.seen_surfaces.len() != index.surfaces.len() {
            self.seen_surfaces.resize(index.surfaces.len(), 0);
            reset_generation = true;
        }
        if reset_generation {
            self.seen.fill(0);
            self.seen_surfaces.fill(0);
            self.generation = 0;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen.fill(0);
            self.generation = 1;
        }

        let generation = self.generation;
        for collider in &index.large_colliders {
            self.mark_candidate(index, *collider, generation);
        }
        for surface in &index.large_surfaces {
            self.mark_surface_candidate(index, *surface, generation);
        }

        let horizontal_reach = PLAYER_MOVE_SPEED * delta_seconds + PLAYER_RADIUS;
        let fall_reach = MAX_FALL_SPEED * delta_seconds + GRAVITY * delta_seconds * delta_seconds;
        let rise_reach = PLAYER_JUMP_SPEED * delta_seconds
            + GRAVITY * delta_seconds * delta_seconds
            + MAX_STEP_UP;
        let min = [
            state.position[0] - horizontal_reach,
            state.position[1] - fall_reach,
            state.position[2] - horizontal_reach,
        ];
        let max = [
            state.position[0] + horizontal_reach,
            state.position[1] + PLAYER_HEIGHT + rise_reach,
            state.position[2] + horizontal_reach,
        ];

        if min.iter().chain(max.iter()).any(|value| !value.is_finite()) {
            stats.fallback_queries = 1;
            for candidate in 0..index.colliders.len() {
                self.mark_candidate(index, candidate, generation);
            }
            for candidate in 0..index.surfaces.len() {
                self.mark_surface_candidate(index, candidate, generation);
            }
            return self.finish_fill(stats);
        }

        let min_cell = cell_for(min);
        let max_cell = cell_for(max);
        if cell_volume(min_cell, max_cell) > MAX_QUERY_CELLS {
            stats.fallback_queries = 1;
            for candidate in 0..index.colliders.len() {
                self.mark_candidate(index, candidate, generation);
            }
            for candidate in 0..index.surfaces.len() {
                self.mark_surface_candidate(index, candidate, generation);
            }
            return self.finish_fill(stats);
        }

        for z in min_cell.2..=max_cell.2 {
            for y in min_cell.1..=max_cell.1 {
                for x in min_cell.0..=max_cell.0 {
                    stats.cell_visits = stats.cell_visits.saturating_add(1);
                    if let Some(candidates) = index.cells.get(&Cell(x, y, z)) {
                        for candidate in candidates {
                            self.mark_candidate(index, *candidate, generation);
                        }
                    }
                    if let Some(candidates) = index.surface_cells.get(&Cell(x, y, z)) {
                        for candidate in candidates {
                            self.mark_surface_candidate(index, *candidate, generation);
                        }
                    }
                }
            }
        }
        self.finish_fill(stats)
    }

    pub fn candidates(&self) -> &[Aabb] {
        &self.candidates
    }

    pub fn surface_candidates(&self) -> &[MapTriangle] {
        &self.surface_candidates
    }

    fn mark_candidate(&mut self, index: &CollisionIndex, candidate: usize, generation: u32) {
        if self.seen[candidate] != generation {
            self.seen[candidate] = generation;
            self.candidates.push(index.colliders[candidate]);
        }
    }

    fn mark_surface_candidate(
        &mut self,
        index: &CollisionIndex,
        candidate: usize,
        generation: u32,
    ) {
        if self.seen_surfaces[candidate] != generation {
            self.seen_surfaces[candidate] = generation;
            self.surface_candidates.push(index.surfaces[candidate]);
        }
    }

    fn finish_fill(&self, mut stats: CollisionQueryStats) -> CollisionQueryStats {
        stats.aabb_candidates = self.candidates.len() as u64;
        stats.triangle_candidates = self.surface_candidates.len() as u64;
        stats.max_cell_visits = stats.cell_visits;
        stats.max_aabb_candidates = stats.aabb_candidates;
        stats.max_triangle_candidates = stats.triangle_candidates;
        stats
    }
}

fn cell_volume(min: Cell, max: Cell) -> u64 {
    let width = (i64::from(max.0) - i64::from(min.0) + 1).max(0) as u64;
    let height = (i64::from(max.1) - i64::from(min.1) + 1).max(0) as u64;
    let depth = (i64::from(max.2) - i64::from(min.2) + 1).max(0) as u64;
    width.saturating_mul(height).saturating_mul(depth)
}

const MAX_MAP_BOXES: usize = 4096;
const MAX_MAP_TRIANGLES: usize = 8192;
const MAX_MAP_MESHES: usize = 64;
const MAX_MAP_COORDINATE: f32 = 1_000_000.0;
const MAX_MAP_BYTES: usize = 1024 * 1024;
const MAX_MESH_BYTES: usize = 8 * 1024 * 1024;
const MAX_MAP_TOTAL_MESH_BYTES: usize = 16 * 1024 * 1024;
const MAX_MESH_VERTICES: usize = 262_144;
const MAX_OBJ_FACE_VERTICES: usize = 128;

#[derive(Clone, Copy, Debug)]
pub struct MapBox {
    pub center: [f32; 3],
    pub size: [f32; 3],
    pub color: [f32; 3],
    pub solid: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct MapTriangle {
    pub vertices: [[f32; 3]; 3],
    pub color: [f32; 3],
    pub solid: bool,
}

#[derive(Clone, Debug)]
pub struct WorldMap {
    pub spawn: [f32; 3],
    boxes: Vec<MapBox>,
    triangles: Vec<MapTriangle>,
    fingerprint: u64,
}

impl WorldMap {
    pub fn load_file(path: &Path) -> io::Result<Self> {
        let mut input = String::new();
        File::open(path)?
            .take((MAX_MAP_BYTES + 1) as u64)
            .read_to_string(&mut input)?;
        let base_dir = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        Self::parse_with_base(&input, Some(base_dir))
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn parse(source: &str) -> Result<Self, String> {
        Self::parse_with_base(source, None)
    }

    fn parse_with_base(source: &str, base_dir: Option<&Path>) -> Result<Self, String> {
        if source.len() > MAX_MAP_BYTES {
            return Err(format!("map exceeds {MAX_MAP_BYTES} bytes"));
        }
        let mut content_lines = source.lines().enumerate().filter_map(|(index, line)| {
            let content = line.split('#').next().unwrap_or("").trim();
            (!content.is_empty()).then_some((index + 1, content))
        });
        let Some((header_line, header)) = content_lines.next() else {
            return Err("map is empty".into());
        };
        let format_version = match header {
            "MINIMIUM_MAP 1" => 1,
            "MINIMIUM_MAP 2" => 2,
            _ => {
                return Err(format!(
                    "line {header_line}: expected `MINIMIUM_MAP 1` or `MINIMIUM_MAP 2`"
                ))
            }
        };

        let mut spawn = None;
        let mut boxes = Vec::new();
        let mut triangles = Vec::new();
        let mut mesh_count = 0usize;
        let mut total_mesh_bytes = 0usize;
        for (line_number, line) in content_lines {
            let fields: Vec<_> = line.split_whitespace().collect();
            match fields.first().copied() {
                Some("spawn") if fields.len() == 4 => {
                    if spawn.is_some() {
                        return Err(format!("line {line_number}: duplicate spawn"));
                    }
                    spawn = Some(parse_map_vec3(&fields[1..4], line_number, "spawn")?);
                }
                Some("box") if fields.len() == 11 => {
                    if boxes.len() >= MAX_MAP_BOXES {
                        return Err(format!(
                            "line {line_number}: map exceeds {MAX_MAP_BOXES} boxes"
                        ));
                    }
                    let center = parse_map_vec3(&fields[1..4], line_number, "box center")?;
                    let size = parse_map_vec3(&fields[4..7], line_number, "box size")?;
                    let color = parse_map_vec3(&fields[7..10], line_number, "box color")?;
                    if size.iter().any(|value| *value <= 0.0) {
                        return Err(format!("line {line_number}: box sizes must be positive"));
                    }
                    if color.iter().any(|value| !(0.0..=1.0).contains(value)) {
                        return Err(format!("line {line_number}: box colors must be in 0..=1"));
                    }
                    let solid = match fields[10] {
                        "solid" => true,
                        "visual" => false,
                        _ => {
                            return Err(format!("line {line_number}: expected `solid` or `visual`"))
                        }
                    };
                    boxes.push(MapBox {
                        center,
                        size,
                        color,
                        solid,
                    });
                }
                Some("triangle")
                    if format_version >= 2 && (fields.len() == 13 || fields.len() == 14) =>
                {
                    if triangles.len() >= MAX_MAP_TRIANGLES {
                        return Err(format!(
                            "line {line_number}: map exceeds {MAX_MAP_TRIANGLES} triangles"
                        ));
                    }
                    let vertices = [
                        parse_map_vec3(&fields[1..4], line_number, "triangle vertex")?,
                        parse_map_vec3(&fields[4..7], line_number, "triangle vertex")?,
                        parse_map_vec3(&fields[7..10], line_number, "triangle vertex")?,
                    ];
                    let color = parse_map_vec3(&fields[10..13], line_number, "triangle color")?;
                    if color.iter().any(|value| !(0.0..=1.0).contains(value)) {
                        return Err(format!(
                            "line {line_number}: triangle colors must be in 0..=1"
                        ));
                    }
                    let area_squared = triangle_area_squared(vertices);
                    if area_squared <= 1.0e-12 {
                        return Err(format!(
                            "line {line_number}: triangle vertices are degenerate"
                        ));
                    }
                    let solid = if fields.len() == 14 {
                        match fields[13] {
                            "solid" => true,
                            "visual" => false,
                            _ => {
                                return Err(format!(
                                    "line {line_number}: expected `solid` or `visual`"
                                ))
                            }
                        }
                    } else {
                        false
                    };
                    triangles.push(MapTriangle {
                        vertices,
                        color,
                        solid,
                    });
                }
                Some("triangle") => {
                    return Err(format!(
                        "line {line_number}: invalid `triangle` record; triangles require map version 2"
                    ));
                }
                Some("mesh") if format_version < 2 => {
                    return Err(format!(
                        "line {line_number}: mesh records require map version 2"
                    ));
                }
                Some("mesh") if fields.len() == 12 => {
                    if mesh_count >= MAX_MAP_MESHES {
                        return Err(format!(
                            "line {line_number}: map exceeds {MAX_MAP_MESHES} mesh assets"
                        ));
                    }
                    let Some(base_dir) = base_dir else {
                        return Err(format!(
                            "line {line_number}: mesh records require file-based map loading"
                        ));
                    };
                    let asset_path = parse_mesh_asset_path(fields[1], line_number)?;
                    let translation =
                        parse_map_vec3(&fields[2..5], line_number, "mesh translation")?;
                    let scale = parse_map_vec3(&fields[5..8], line_number, "mesh scale")?;
                    let color = parse_map_vec3(&fields[8..11], line_number, "mesh color")?;
                    if scale.iter().any(|value| *value <= 0.0) {
                        return Err(format!(
                            "line {line_number}: mesh scale components must be positive"
                        ));
                    }
                    if color.iter().any(|value| !(0.0..=1.0).contains(value)) {
                        return Err(format!("line {line_number}: mesh colors must be in 0..=1"));
                    }
                    let solid = match fields[11] {
                        "solid" => true,
                        "visual" => false,
                        _ => {
                            return Err(format!("line {line_number}: expected `solid` or `visual`"))
                        }
                    };
                    let remaining_mesh_bytes =
                        MAX_MAP_TOTAL_MESH_BYTES.saturating_sub(total_mesh_bytes);
                    let mesh_bytes = load_obj_triangles(
                        base_dir,
                        &asset_path,
                        ObjMeshOptions {
                            translation,
                            scale,
                            color,
                            solid,
                        },
                        remaining_mesh_bytes,
                        &mut triangles,
                    )
                    .map_err(|error| format!("line {line_number}: {error}"))?;
                    total_mesh_bytes = total_mesh_bytes.saturating_add(mesh_bytes);
                    mesh_count += 1;
                }
                Some("mesh") => {
                    return Err(format!(
                        "line {line_number}: invalid `mesh` record; expected `mesh PATH TX TY TZ SX SY SZ R G B solid|visual`"
                    ));
                }
                Some("spawn") | Some("box") => {
                    return Err(format!(
                        "line {line_number}: invalid `{}` record",
                        fields[0]
                    ));
                }
                Some(_) => return Err(format!("line {line_number}: unknown map record")),
                None => unreachable!(),
            }
        }

        let spawn = spawn.ok_or("map is missing a spawn record")?;
        if boxes.is_empty() && triangles.is_empty() {
            return Err("map must contain at least one box or triangle".into());
        }
        if boxes.iter().filter(|map_box| map_box.solid).any(|map_box| {
            intersects_player(spawn, Aabb::from_center_size(map_box.center, map_box.size))
        }) || triangles
            .iter()
            .filter(|triangle| triangle.solid)
            .any(|triangle| triangle_intersects_player(spawn, *triangle))
        {
            return Err("spawn overlaps solid map geometry".into());
        }
        let fingerprint = map_fingerprint(spawn, &boxes, &triangles);
        Ok(Self {
            spawn,
            boxes,
            triangles,
            fingerprint,
        })
    }

    pub fn boxes(&self) -> &[MapBox] {
        &self.boxes
    }

    pub fn triangles(&self) -> &[MapTriangle] {
        &self.triangles
    }

    pub fn solid_colliders(&self) -> Vec<Aabb> {
        self.boxes
            .iter()
            .filter(|map_box| map_box.solid)
            .map(|map_box| Aabb::from_center_size(map_box.center, map_box.size))
            .collect()
    }

    pub fn solid_triangles(&self) -> Vec<MapTriangle> {
        self.triangles
            .iter()
            .copied()
            .filter(|triangle| triangle.solid)
            .collect()
    }

    pub fn collision_index(&self) -> CollisionIndex {
        CollisionIndex::from_geometry(self.solid_colliders(), self.solid_triangles())
    }

    pub fn can_place_player(&self, position: [f32; 3]) -> bool {
        position.iter().all(|value| value.is_finite())
            && self
                .boxes
                .iter()
                .filter(|map_box| map_box.solid)
                .all(|map_box| {
                    !intersects_player(
                        position,
                        Aabb::from_center_size(map_box.center, map_box.size),
                    )
                })
            && self
                .triangles
                .iter()
                .filter(|triangle| triangle.solid)
                .all(|triangle| !triangle_intersects_player(position, *triangle))
    }

    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }
}

fn map_fingerprint(spawn: [f32; 3], boxes: &[MapBox], triangles: &[MapTriangle]) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    };
    feed(b"MINIMIUM_MAP 1");
    for value in spawn {
        feed(&canonical_float_bits(value).to_le_bytes());
    }
    for map_box in boxes {
        for value in map_box
            .center
            .into_iter()
            .chain(map_box.size)
            .chain(map_box.color)
        {
            feed(&canonical_float_bits(value).to_le_bytes());
        }
        feed(&[u8::from(map_box.solid)]);
    }
    if !triangles.is_empty() {
        feed(b"\0TRIANGLES\0");
        feed(&(triangles.len() as u32).to_le_bytes());
        for triangle in triangles {
            for value in triangle
                .vertices
                .into_iter()
                .flatten()
                .chain(triangle.color)
            {
                feed(&canonical_float_bits(value).to_le_bytes());
            }
            feed(&[u8::from(triangle.solid)]);
        }
    }
    hash
}

fn triangle_area_squared(vertices: [[f32; 3]; 3]) -> f32 {
    let cross = triangle_normal(vertices);
    cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]
}

fn triangle_normal(vertices: [[f32; 3]; 3]) -> [f32; 3] {
    let a = vertices[0];
    let b = vertices[1];
    let c = vertices[2];
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ]
}

fn is_valid_solid_triangle(surface: MapTriangle) -> bool {
    if !surface.solid
        || surface
            .vertices
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
    {
        return false;
    }
    let normal = triangle_normal(surface.vertices);
    if normal.iter().any(|value| !value.is_finite()) {
        return false;
    }
    let normal_length_squared =
        normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2];
    normal_length_squared.is_finite() && normal_length_squared > 1.0e-12
}

fn is_walkable_triangle(surface: MapTriangle) -> bool {
    if !is_valid_solid_triangle(surface) {
        return false;
    }
    let normal = triangle_normal(surface.vertices);
    let normal_length_squared =
        normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2];
    normal[1].abs() >= normal_length_squared.sqrt() * 0.5
}

fn triangle_height_for_player(position: [f32; 3], surface: MapTriangle) -> Option<f32> {
    if !is_walkable_triangle(surface) {
        return None;
    }
    let [a, b, c] = surface.vertices;
    let v0 = [c[0] - a[0], c[2] - a[2]];
    let v1 = [b[0] - a[0], b[2] - a[2]];
    let dot00 = v0[0] * v0[0] + v0[1] * v0[1];
    let dot01 = v0[0] * v1[0] + v0[1] * v1[1];
    let dot11 = v1[0] * v1[0] + v1[1] * v1[1];
    let denominator = dot00 * dot11 - dot01 * dot01;
    if !denominator.is_finite() || denominator.abs() <= 1.0e-12 {
        return None;
    }
    let inverse_denominator = denominator.recip();
    let normal = triangle_normal(surface.vertices);
    if normal[1].abs() <= 1.0e-6 {
        return None;
    }
    let sample_points = [
        [position[0], position[2]],
        [position[0] - PLAYER_RADIUS, position[2]],
        [position[0] + PLAYER_RADIUS, position[2]],
        [position[0], position[2] - PLAYER_RADIUS],
        [position[0], position[2] + PLAYER_RADIUS],
    ];
    let mut highest = None;
    for point in sample_points {
        let v2 = [point[0] - a[0], point[1] - a[2]];
        let dot02 = v0[0] * v2[0] + v0[1] * v2[1];
        let dot12 = v1[0] * v2[0] + v1[1] * v2[1];
        let u = (dot11 * dot02 - dot01 * dot12) * inverse_denominator;
        let v = (dot00 * dot12 - dot01 * dot02) * inverse_denominator;
        if u < -1.0e-5 || v < -1.0e-5 || u + v > 1.00001 {
            continue;
        }
        let height =
            a[1] - (normal[0] * (point[0] - a[0]) + normal[2] * (point[1] - a[2])) / normal[1];
        if height.is_finite() && highest.is_none_or(|current| height > current) {
            highest = Some(height);
        }
    }
    highest
}

fn highest_triangle_height_for_player(position: [f32; 3], surfaces: &[MapTriangle]) -> Option<f32> {
    surfaces
        .iter()
        .copied()
        .filter_map(|surface| triangle_height_for_player(position, surface))
        .max_by(f32::total_cmp)
}

fn intersects_triangle_surface(position: [f32; 3], surface: MapTriangle) -> bool {
    triangle_height_for_player(position, surface).is_some_and(|height| {
        position[1] < height - COLLISION_EPSILON
            && position[1] + PLAYER_HEIGHT > height + COLLISION_EPSILON
    })
}

fn triangle_intersects_player(position: [f32; 3], surface: MapTriangle) -> bool {
    if is_walkable_triangle(surface) {
        return intersects_triangle_surface(position, surface);
    }
    if !is_valid_solid_triangle(surface) {
        return false;
    }

    let center = [position[0], position[1] + PLAYER_HEIGHT * 0.5, position[2]];
    let half = [PLAYER_RADIUS, PLAYER_HEIGHT * 0.5, PLAYER_RADIUS];
    let vertices = surface.vertices.map(|vertex| {
        [
            vertex[0] - center[0],
            vertex[1] - center[1],
            vertex[2] - center[2],
        ]
    });

    for axis in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        if triangle_separated_on_axis(vertices, half, axis) {
            return false;
        }
    }

    let edges = [
        vector_subtract(vertices[1], vertices[0]),
        vector_subtract(vertices[2], vertices[1]),
        vector_subtract(vertices[0], vertices[2]),
    ];
    let normal = triangle_normal(surface.vertices);
    if triangle_separated_on_axis(vertices, half, normal) {
        return false;
    }
    for edge in edges {
        for axis in [
            [0.0, edge[2], -edge[1]],
            [-edge[2], 0.0, edge[0]],
            [edge[1], -edge[0], 0.0],
        ] {
            if triangle_separated_on_axis(vertices, half, axis) {
                return false;
            }
        }
    }
    true
}

fn clip_vertical_motion_to_triangles(
    position: [f32; 3],
    old_y: f32,
    new_y: f32,
    surfaces: &[MapTriangle],
) -> Option<f32> {
    let change = new_y - old_y;
    if !change.is_finite() || change == 0.0 {
        return None;
    }
    let steps = (change.abs() / PLAYER_RADIUS).ceil().clamp(1.0, 16.0) as u32;
    let mut previous_y = old_y;
    for step in 1..=steps {
        let fraction = step as f32 / steps as f32;
        let candidate_y = old_y + change * fraction;
        let mut candidate_position = position;
        candidate_position[1] = candidate_y;
        if surfaces.iter().copied().any(|surface| {
            !is_walkable_triangle(surface)
                && triangle_intersects_player(candidate_position, surface)
        }) {
            return Some(previous_y);
        }
        previous_y = candidate_y;
    }
    None
}

fn triangle_separated_on_axis(vertices: [[f32; 3]; 3], half: [f32; 3], axis: [f32; 3]) -> bool {
    let axis_length_squared = axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2];
    if !axis_length_squared.is_finite() || axis_length_squared <= 1.0e-12 {
        return false;
    }
    let projection =
        |vertex: [f32; 3]| vertex[0] * axis[0] + vertex[1] * axis[1] + vertex[2] * axis[2];
    let first = projection(vertices[0]);
    let second = projection(vertices[1]);
    let third = projection(vertices[2]);
    let minimum = first.min(second).min(third);
    let maximum = first.max(second).max(third);
    let radius = half[0] * axis[0].abs() + half[1] * axis[1].abs() + half[2] * axis[2].abs();
    let margin = COLLISION_EPSILON * axis_length_squared.sqrt();
    minimum >= radius - margin || maximum <= -radius + margin
}

fn vector_subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn parse_mesh_asset_path(value: &str, line_number: usize) -> Result<std::path::PathBuf, String> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "line {line_number}: mesh path must be relative and cannot contain `.` or `..` components"
        ));
    }
    Ok(path.to_path_buf())
}

#[derive(Clone, Copy)]
struct ObjMeshOptions {
    translation: [f32; 3],
    scale: [f32; 3],
    color: [f32; 3],
    solid: bool,
}

fn load_obj_triangles(
    base_dir: &Path,
    asset_path: &Path,
    options: ObjMeshOptions,
    remaining_map_bytes: usize,
    triangles: &mut Vec<MapTriangle>,
) -> Result<usize, String> {
    let ObjMeshOptions {
        translation,
        scale,
        color,
        solid,
    } = options;
    let canonical_base = std::fs::canonicalize(base_dir)
        .map_err(|error| format!("cannot resolve map directory: {error}"))?;
    let joined_path = canonical_base.join(asset_path);
    let canonical_asset = std::fs::canonicalize(&joined_path).map_err(|error| {
        format!(
            "cannot resolve mesh asset `{}`: {error}",
            asset_path.display()
        )
    })?;
    if !canonical_asset.starts_with(&canonical_base) {
        return Err(format!(
            "mesh asset `{}` resolves outside the map directory",
            asset_path.display()
        ));
    }

    let mut source = String::new();
    let byte_limit = MAX_MESH_BYTES.min(remaining_map_bytes);
    if byte_limit == 0 {
        return Err(format!(
            "map mesh assets exceed {MAX_MAP_TOTAL_MESH_BYTES} bytes"
        ));
    }
    File::open(&canonical_asset)
        .and_then(|file| {
            file.take((byte_limit + 1) as u64)
                .read_to_string(&mut source)
        })
        .map_err(|error| format!("cannot read mesh asset `{}`: {error}", asset_path.display()))?;
    if source.len() > byte_limit {
        if remaining_map_bytes < MAX_MESH_BYTES {
            return Err(format!(
                "map mesh assets exceed {MAX_MAP_TOTAL_MESH_BYTES} bytes"
            ));
        }
        return Err(format!(
            "mesh asset `{}` exceeds {MAX_MESH_BYTES} bytes",
            asset_path.display()
        ));
    }

    let mut vertices = Vec::<[f32; 3]>::with_capacity(1024);
    let mut added_triangles = 0usize;
    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        let content = line.split('#').next().unwrap_or("").trim();
        if content.is_empty() {
            continue;
        }
        let fields: Vec<_> = content.split_whitespace().collect();
        match fields[0] {
            "v" if fields.len() >= 4 => {
                if vertices.len() >= MAX_MESH_VERTICES {
                    return Err(format!(
                        "mesh asset `{}` exceeds {MAX_MESH_VERTICES} vertices",
                        asset_path.display()
                    ));
                }
                let mut vertex = [0.0; 3];
                for axis in 0..3 {
                    vertex[axis] = fields[axis + 1].parse::<f32>().map_err(|_| {
                        format!(
                            "mesh asset `{}` line {line_number}: invalid vertex coordinate",
                            asset_path.display()
                        )
                    })?;
                    if !vertex[axis].is_finite() || vertex[axis].abs() > MAX_MAP_COORDINATE {
                        return Err(format!(
                            "mesh asset `{}` line {line_number}: vertex is outside supported range",
                            asset_path.display()
                        ));
                    }
                }
                vertices.push(vertex);
            }
            "v" => {
                return Err(format!(
                    "mesh asset `{}` line {line_number}: vertex requires three coordinates",
                    asset_path.display()
                ));
            }
            "f" if fields.len() >= 4 => {
                let face_vertex_count = fields.len() - 1;
                if face_vertex_count > MAX_OBJ_FACE_VERTICES {
                    return Err(format!(
                        "mesh asset `{}` line {line_number}: face exceeds {MAX_OBJ_FACE_VERTICES} vertices",
                        asset_path.display()
                    ));
                }
                let mut polygon = Vec::with_capacity(face_vertex_count);
                for field in fields.iter().skip(1) {
                    polygon.push(parse_obj_vertex_index(field, vertices.len()).map_err(|_| {
                        format!(
                            "mesh asset `{}` line {line_number}: invalid face vertex index",
                            asset_path.display()
                        )
                    })?);
                }
                let face_triangles =
                    triangulate_obj_face(&vertices, &polygon).map_err(|reason| {
                        format!(
                        "mesh asset `{}` line {line_number}: cannot triangulate face ({reason})",
                        asset_path.display()
                    )
                    })?;
                for face_triangle in face_triangles {
                    if triangles.len() >= MAX_MAP_TRIANGLES {
                        return Err(format!("map exceeds {MAX_MAP_TRIANGLES} triangles"));
                    }
                    let local_vertices = face_triangle.map(|index| vertices[index]);
                    let world_vertices = local_vertices.map(|vertex| {
                        [
                            vertex[0] * scale[0] + translation[0],
                            vertex[1] * scale[1] + translation[1],
                            vertex[2] * scale[2] + translation[2],
                        ]
                    });
                    if world_vertices
                        .iter()
                        .flatten()
                        .any(|value| !value.is_finite() || value.abs() > MAX_MAP_COORDINATE)
                    {
                        return Err(format!(
                            "mesh asset `{}` line {line_number}: transformed vertex is outside supported range",
                            asset_path.display()
                        ));
                    }
                    if triangle_area_squared(world_vertices) <= 1.0e-12 {
                        return Err(format!(
                            "mesh asset `{}` line {line_number}: face creates a degenerate triangle",
                            asset_path.display()
                        ));
                    }
                    triangles.push(MapTriangle {
                        vertices: world_vertices,
                        color,
                        solid,
                    });
                    added_triangles += 1;
                }
            }
            "f" => {
                return Err(format!(
                    "mesh asset `{}` line {line_number}: face requires at least three vertices",
                    asset_path.display()
                ));
            }
            "vt" | "vn" | "o" | "g" | "s" | "usemtl" | "mtllib" | "l" | "p" => {}
            _ => {
                return Err(format!(
                    "mesh asset `{}` line {line_number}: unsupported OBJ record `{}`",
                    asset_path.display(),
                    fields[0]
                ));
            }
        }
    }
    if added_triangles == 0 {
        return Err(format!(
            "mesh asset `{}` contains no faces",
            asset_path.display()
        ));
    }
    Ok(source.len())
}

fn parse_obj_vertex_index(field: &str, vertex_count: usize) -> Result<usize, ()> {
    let raw = field
        .split('/')
        .next()
        .unwrap_or("")
        .parse::<i64>()
        .map_err(|_| ())?;
    if raw == 0 {
        return Err(());
    }
    let resolved = if raw > 0 {
        raw - 1
    } else {
        i64::try_from(vertex_count).map_err(|_| ())? + raw
    };
    usize::try_from(resolved)
        .ok()
        .filter(|index| *index < vertex_count)
        .ok_or(())
}

fn triangulate_obj_face(
    vertices: &[[f32; 3]],
    face: &[usize],
) -> Result<Vec<[usize; 3]>, &'static str> {
    if face.len() < 3 {
        return Err("face has fewer than three vertices");
    }

    let mut normal = [0.0f64; 3];
    for index in 0..face.len() {
        let current = vertices[face[index]].map(f64::from);
        let next = vertices[face[(index + 1) % face.len()]].map(f64::from);
        normal[0] += (current[1] - next[1]) * (current[2] + next[2]);
        normal[1] += (current[2] - next[2]) * (current[0] + next[0]);
        normal[2] += (current[0] - next[0]) * (current[1] + next[1]);
    }
    let normal_length =
        (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if !normal_length.is_finite() || normal_length <= 1.0e-12 {
        return Err("face has no stable plane");
    }
    let unit_normal = normal.map(|value| value / normal_length);
    let anchor = vertices[face[0]].map(f64::from);
    let mut extent: f64 = 0.0;
    for index in face {
        let vertex = vertices[*index].map(f64::from);
        let delta = [
            vertex[0] - anchor[0],
            vertex[1] - anchor[1],
            vertex[2] - anchor[2],
        ];
        extent = extent
            .max(delta[0].abs())
            .max(delta[1].abs())
            .max(delta[2].abs());
        let distance =
            delta[0] * unit_normal[0] + delta[1] * unit_normal[1] + delta[2] * unit_normal[2];
        if distance.abs() > 1.0e-4 * extent.max(1.0) {
            return Err("face is not planar");
        }
    }

    let drop_axis = if unit_normal[0].abs() >= unit_normal[1].abs()
        && unit_normal[0].abs() >= unit_normal[2].abs()
    {
        0
    } else if unit_normal[1].abs() >= unit_normal[2].abs() {
        1
    } else {
        2
    };
    let projected: Vec<[f64; 2]> = face
        .iter()
        .map(|index| {
            let vertex = vertices[*index].map(f64::from);
            match drop_axis {
                0 => [vertex[1], vertex[2]],
                1 => [vertex[0], vertex[2]],
                _ => [vertex[0], vertex[1]],
            }
        })
        .collect();
    let scale = extent.max(1.0);
    let epsilon = scale * scale * 1.0e-12;

    for first in 0..projected.len() {
        for second in first + 1..projected.len() {
            let delta_x = projected[first][0] - projected[second][0];
            let delta_y = projected[first][1] - projected[second][1];
            if delta_x * delta_x + delta_y * delta_y <= epsilon {
                return Err("face contains duplicate vertices");
            }
        }
    }

    let signed_area: f64 = (0..projected.len())
        .map(|index| {
            let current = projected[index];
            let next = projected[(index + 1) % projected.len()];
            current[0] * next[1] - next[0] * current[1]
        })
        .sum();
    if signed_area.abs() <= epsilon {
        return Err("face has zero area");
    }
    let orientation = signed_area.signum();

    let mut active: Vec<usize> = (0..face.len()).collect();
    let mut removed_collinear = true;
    while removed_collinear && active.len() > 3 {
        removed_collinear = false;
        for cursor in 0..active.len() {
            let previous = active[(cursor + active.len() - 1) % active.len()];
            let current = active[cursor];
            let next = active[(cursor + 1) % active.len()];
            let incoming = [
                projected[current][0] - projected[previous][0],
                projected[current][1] - projected[previous][1],
            ];
            let outgoing = [
                projected[next][0] - projected[current][0],
                projected[next][1] - projected[current][1],
            ];
            let turn = incoming[0] * outgoing[1] - incoming[1] * outgoing[0];
            if turn.abs() <= epsilon {
                let direction = incoming[0] * outgoing[0] + incoming[1] * outgoing[1];
                if direction < -epsilon {
                    return Err("face boundary doubles back");
                }
                active.remove(cursor);
                removed_collinear = true;
                break;
            }
        }
    }

    let mut result = Vec::with_capacity(active.len().saturating_sub(2));
    while active.len() > 3 {
        let mut clipped = false;
        for cursor in 0..active.len() {
            let previous = active[(cursor + active.len() - 1) % active.len()];
            let current = active[cursor];
            let next = active[(cursor + 1) % active.len()];
            let a = projected[previous];
            let b = projected[current];
            let c = projected[next];
            let turn = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
            if turn * orientation <= epsilon {
                continue;
            }

            let contains_vertex = active.iter().copied().any(|candidate| {
                if candidate == previous || candidate == current || candidate == next {
                    return false;
                }
                let point = projected[candidate];
                let ab = (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0]);
                let bc = (c[0] - b[0]) * (point[1] - b[1]) - (c[1] - b[1]) * (point[0] - b[0]);
                let ca = (a[0] - c[0]) * (point[1] - c[1]) - (a[1] - c[1]) * (point[0] - c[0]);
                ab * orientation >= -epsilon
                    && bc * orientation >= -epsilon
                    && ca * orientation >= -epsilon
            });
            if contains_vertex {
                continue;
            }

            result.push([face[previous], face[current], face[next]]);
            active.remove(cursor);
            clipped = true;
            break;
        }
        if !clipped {
            return Err("face is self-intersecting or numerically unstable");
        }
    }
    let last = [face[active[0]], face[active[1]], face[active[2]]];
    let a = projected[active[0]];
    let b = projected[active[1]];
    let c = projected[active[2]];
    let final_turn = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    if final_turn * orientation <= epsilon {
        return Err("remaining polygon is degenerate");
    }
    result.push(last);
    Ok(result)
}

fn canonical_float_bits(value: f32) -> u32 {
    if value == 0.0 {
        0.0f32.to_bits()
    } else {
        value.to_bits()
    }
}

fn parse_map_vec3(fields: &[&str], line_number: usize, name: &str) -> Result<[f32; 3], String> {
    let mut values = [0.0; 3];
    for (index, field) in fields.iter().enumerate() {
        values[index] = field
            .parse::<f32>()
            .map_err(|_| format!("line {line_number}: invalid {name}"))?;
        if !values[index].is_finite() || values[index].abs() > MAX_MAP_COORDINATE {
            return Err(format!(
                "line {line_number}: {name} is outside supported range"
            ));
        }
    }
    Ok(values)
}

#[derive(Clone)]
pub struct WorldSnapshot {
    players: Arc<HashMap<u64, Player>>,
    cells: Arc<HashMap<Cell, Vec<u64>>>,
}

#[derive(Clone, Copy)]
struct NearbyCandidate<'a> {
    player: &'a Player,
    distance_squared: f32,
}

impl WorldSnapshot {
    pub fn position(&self, entity: u64) -> Option<[f32; 3]> {
        self.players.get(&entity).map(|player| player.position)
    }

    pub fn character_state(&self, entity: u64) -> Option<CharacterState> {
        self.players.get(&entity).map(|player| CharacterState {
            position: player.position,
            velocity: player.velocity,
            grounded: player.grounded,
        })
    }

    pub fn append_nearby(&self, recipient: u64, limit: usize, output: &mut String) {
        let mut append = |player: &Player| {
            let _ = write!(
                output,
                " {} {} {:.2} {:.2} {:.2}",
                player.id, player.name, player.position[0], player.position[1], player.position[2]
            );
        };
        if limit <= 16 {
            self.visit_nearby::<16>(recipient, limit, &mut append);
        } else {
            for player in self.nearby_players(recipient, limit) {
                append(&player);
            }
        }
    }

    pub fn visit_nearby<const CAPACITY: usize>(
        &self,
        recipient: u64,
        limit: usize,
        mut visit: impl FnMut(&Player),
    ) {
        let selection_limit = limit.min(CAPACITY);
        if selection_limit == 0 {
            return;
        }

        let mut nearest: [Option<NearbyCandidate<'_>>; CAPACITY] = [None; CAPACITY];
        let mut nearest_count = 0;
        self.for_each_nearby_candidate(recipient, |player, distance_squared| {
            let candidate = NearbyCandidate {
                player,
                distance_squared,
            };
            let insert_at = if nearest_count < selection_limit {
                let index = nearest_count;
                nearest_count += 1;
                index
            } else {
                let last = nearest[selection_limit - 1].expect("nearest entries stay packed");
                if nearby_candidate_precedes(candidate, last) {
                    selection_limit - 1
                } else {
                    return;
                }
            };
            nearest[insert_at] = Some(candidate);
            let mut index = insert_at;
            while index > 0 {
                let previous = nearest[index - 1].expect("nearest entries stay packed");
                let current = nearest[index].expect("inserted candidate is present");
                if !nearby_candidate_precedes(current, previous) {
                    break;
                }
                nearest[index] = Some(previous);
                nearest[index - 1] = Some(current);
                index -= 1;
            }
        });

        for candidate in nearest.into_iter().take(nearest_count).flatten() {
            visit(candidate.player);
        }
    }

    pub fn nearby_players(&self, recipient: u64, limit: usize) -> Vec<Player> {
        if limit == 0 {
            return Vec::new();
        }
        let mut candidates = Vec::with_capacity(limit.min(self.players.len()));
        self.for_each_nearby_candidate(recipient, |player, distance_squared| {
            candidates.push((distance_squared, player.id, player));
        });
        candidates.sort_unstable_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
        });
        candidates
            .into_iter()
            .take(limit)
            .map(|(_, _, player)| player.clone())
            .collect()
    }

    fn for_each_nearby_candidate<'a>(
        &'a self,
        recipient: u64,
        mut visit: impl FnMut(&'a Player, f32),
    ) {
        let Some(origin) = self.players.get(&recipient) else {
            return;
        };
        let minimum = cell_for([
            origin.position[0] - VISIBILITY_RADIUS,
            origin.position[1] - VISIBILITY_RADIUS,
            origin.position[2] - VISIBILITY_RADIUS,
        ]);
        let maximum = cell_for([
            origin.position[0] + VISIBILITY_RADIUS,
            origin.position[1] + VISIBILITY_RADIUS,
            origin.position[2] + VISIBILITY_RADIUS,
        ]);
        let radius_squared = VISIBILITY_RADIUS * VISIBILITY_RADIUS;
        let axis_cell_count = |minimum: i32, maximum: i32| {
            usize::try_from(i64::from(maximum) - i64::from(minimum) + 1).unwrap_or(usize::MAX)
        };
        let query_cell_count = axis_cell_count(minimum.0, maximum.0)
            .saturating_mul(axis_cell_count(minimum.1, maximum.1))
            .saturating_mul(axis_cell_count(minimum.2, maximum.2));
        let mut inspect_ids = |ids: &[u64]| {
            for id in ids {
                if *id == recipient {
                    continue;
                }
                let Some(player) = self.players.get(id) else {
                    continue;
                };
                let distance_squared = (0..3)
                    .map(|axis| {
                        let delta = player.position[axis] - origin.position[axis];
                        delta * delta
                    })
                    .sum::<f32>();
                if distance_squared <= radius_squared {
                    visit(player, distance_squared);
                }
            }
        };

        if self.cells.len() < query_cell_count {
            for (Cell(x, y, z), ids) in self.cells.iter() {
                if *x >= minimum.0
                    && *x <= maximum.0
                    && *y >= minimum.1
                    && *y <= maximum.1
                    && *z >= minimum.2
                    && *z <= maximum.2
                {
                    inspect_ids(ids);
                }
            }
        } else {
            for z in minimum.2..=maximum.2 {
                for y in minimum.1..=maximum.1 {
                    for x in minimum.0..=maximum.0 {
                        if let Some(ids) = self.cells.get(&Cell(x, y, z)) {
                            inspect_ids(ids);
                        }
                    }
                }
            }
        }
    }
}

fn nearby_candidate_precedes(left: NearbyCandidate<'_>, right: NearbyCandidate<'_>) -> bool {
    left.distance_squared
        .total_cmp(&right.distance_squared)
        .then_with(|| left.player.id.cmp(&right.player.id))
        .is_lt()
}

#[derive(Clone, Copy)]
struct PlayerStepInput {
    id: u64,
    state: CharacterState,
    input: CharacterInput,
}

#[derive(Clone, Copy)]
struct PlayerStepResult {
    id: u64,
    state: CharacterState,
}

#[derive(Default)]
struct SimulationBatch {
    inputs: Vec<PlayerStepInput>,
    results: Vec<PlayerStepResult>,
    collision_query: CollisionQuery,
    collision_stats: CollisionQueryStats,
}

#[derive(Default)]
pub struct World {
    players: Arc<HashMap<u64, Player>>,
    cells: Arc<HashMap<Cell, Vec<u64>>>,
    player_cells: Arc<HashMap<u64, Cell>>,
    collision_index: Arc<CollisionIndex>,
    collision_query: CollisionQuery,
    combat_player_query: PlayerRayQuery,
    combat_map_query: MapRayQuery,
    simulation_batches: Vec<SimulationBatch>,
    next_id: u64,
    spawn: [f32; 3],
}

impl World {
    pub fn join(&mut self, name: String) -> u64 {
        self.next_id += 1;
        self.join_as(self.next_id, name);
        self.next_id
    }

    pub fn join_as(&mut self, id: u64, name: String) {
        self.join_at(id, name, self.spawn);
    }

    pub fn join_at(&mut self, id: u64, name: String, position: [f32; 3]) {
        self.join_at_with_health(id, name, position, PLAYER_MAX_HEALTH);
    }

    pub fn join_at_with_health(&mut self, id: u64, name: String, position: [f32; 3], health: u16) {
        self.next_id = self.next_id.max(id);
        let position = if position.iter().all(|value| value.is_finite())
            && self
                .collision_index
                .colliders
                .iter()
                .all(|collider| !intersects_player(position, *collider))
        {
            position
        } else {
            self.spawn
        };
        let player = Player {
            id,
            name: Arc::from(name),
            position,
            velocity: [0.0; 3],
            grounded: false,
            health: health.clamp(1, PLAYER_MAX_HEALTH),
        };
        let cell = cell_for(player.position);
        Arc::make_mut(&mut self.cells)
            .entry(cell)
            .or_default()
            .push(player.id);
        Arc::make_mut(&mut self.player_cells).insert(player.id, cell);
        Arc::make_mut(&mut self.players).insert(id, player);
    }

    pub fn leave(&mut self, id: u64) {
        Arc::make_mut(&mut self.players).remove(&id);
        if let Some(cell) = Arc::make_mut(&mut self.player_cells).remove(&id) {
            let cells = Arc::make_mut(&mut self.cells);
            let empty = if let Some(players) = cells.get_mut(&cell) {
                players.retain(|player| *player != id);
                players.is_empty()
            } else {
                false
            };
            if empty {
                cells.remove(&cell);
            }
        }
    }

    pub fn step_players(
        &mut self,
        inputs: &HashMap<u64, CharacterInput>,
        delta_seconds: f32,
    ) -> CollisionQueryStats {
        let Self {
            players,
            cells,
            player_cells,
            collision_index,
            collision_query,
            ..
        } = self;
        let players = Arc::make_mut(players);
        let cells = Arc::make_mut(cells);
        let player_cells = Arc::make_mut(player_cells);
        let mut collision_stats = CollisionQueryStats::default();
        for (id, player) in players {
            let mut state = CharacterState {
                position: player.position,
                velocity: player.velocity,
                grounded: player.grounded,
            };
            let input = inputs.get(id).copied().unwrap_or_default();
            collision_stats.accumulate(collision_query.fill(collision_index, state, delta_seconds));
            step_character(
                &mut state,
                input,
                collision_query.candidates(),
                collision_query.surface_candidates(),
                delta_seconds,
            );
            player.position = state.position;
            player.velocity = state.velocity;
            player.grounded = state.grounded;
            update_cell_index(cells, player_cells, *id, player.position);
        }
        collision_stats
    }

    pub fn step_players_parallel(
        &mut self,
        inputs: &HashMap<u64, CharacterInput>,
        delta_seconds: f32,
        executor: &Executor,
    ) -> CollisionQueryStats {
        if executor.lane_count() <= 1 || self.players.len() < PARALLEL_SIMULATION_MIN_PLAYERS {
            return self.step_players(inputs, delta_seconds);
        }

        let World {
            players,
            cells,
            player_cells,
            collision_index,
            simulation_batches,
            ..
        } = self;
        let players = Arc::make_mut(players);
        let cells = Arc::make_mut(cells);
        let player_cells = Arc::make_mut(player_cells);
        let batch_count = executor
            .lane_count()
            .min(players.len().div_ceil(PLAYERS_PER_SIMULATION_BATCH))
            .max(2);
        simulation_batches.resize_with(batch_count, SimulationBatch::default);
        let inputs_per_batch = players.len().div_ceil(batch_count);
        for batch in simulation_batches.iter_mut() {
            batch.inputs.clear();
            batch.results.clear();
            batch.collision_stats = CollisionQueryStats::default();
            if batch.inputs.capacity() < inputs_per_batch {
                batch.inputs.reserve(inputs_per_batch);
            }
            if batch.results.capacity() < inputs_per_batch {
                batch.results.reserve(inputs_per_batch);
            }
        }
        for (index, (id, player)) in players.iter().enumerate() {
            let batch_index = (index / inputs_per_batch).min(batch_count - 1);
            simulation_batches[batch_index]
                .inputs
                .push(PlayerStepInput {
                    id: *id,
                    state: CharacterState {
                        position: player.position,
                        velocity: player.velocity,
                        grounded: player.grounded,
                    },
                    input: inputs.get(id).copied().unwrap_or_default(),
                });
        }

        let collision_index = Arc::clone(collision_index);
        let batches = std::mem::take(simulation_batches);
        let processed_batches = executor.map(batches, move |mut batch| {
            for step in batch.inputs.drain(..) {
                let mut state = step.state;
                batch.collision_stats.accumulate(batch.collision_query.fill(
                    &collision_index,
                    state,
                    delta_seconds,
                ));
                step_character(
                    &mut state,
                    step.input,
                    batch.collision_query.candidates(),
                    batch.collision_query.surface_candidates(),
                    delta_seconds,
                );
                batch.results.push(PlayerStepResult { id: step.id, state });
            }
            batch
        });

        let mut collision_stats = CollisionQueryStats::default();
        for batch in &processed_batches {
            collision_stats.accumulate(batch.collision_stats);
            for result in &batch.results {
                if let Some(player) = players.get_mut(&result.id) {
                    player.position = result.state.position;
                    player.velocity = result.state.velocity;
                    player.grounded = result.state.grounded;
                    update_cell_index(cells, player_cells, result.id, player.position);
                }
            }
        }
        *simulation_batches = processed_batches;
        collision_stats
    }

    pub fn set_position(&mut self, id: u64, position: [f32; 3]) -> bool {
        if !position.iter().all(|value| value.is_finite()) {
            return false;
        }
        let Some(player) = Arc::make_mut(&mut self.players).get_mut(&id) else {
            return false;
        };
        player.position = position;
        player.velocity = [0.0; 3];
        player.grounded = false;
        self.update_player_cell(id, position);
        true
    }

    pub fn fire_hitscan(&mut self, attacker_id: u64, direction: [f32; 3]) -> Option<ShotOutcome> {
        if direction.iter().any(|value| !value.is_finite()) {
            return None;
        }
        let length_squared = direction.iter().map(|value| value * value).sum::<f32>();
        if !length_squared.is_finite() || length_squared <= 1.0e-8 {
            return None;
        }
        let inverse_length = length_squared.sqrt().recip();
        let direction = direction.map(|value| value * inverse_length);
        let World {
            players,
            cells,
            player_cells,
            collision_index,
            combat_player_query,
            combat_map_query,
            spawn,
            ..
        } = self;
        let players = Arc::make_mut(players);
        let attacker = players.get(&attacker_id)?;
        let origin = [
            attacker.position[0],
            attacker.position[1] + PLAYER_HEIGHT * 0.9,
            attacker.position[2],
        ];
        combat_player_query.fill(cells, origin, direction, HITSCAN_RANGE);
        combat_map_query.fill(collision_index, origin, direction, HITSCAN_RANGE);

        let mut nearest_player: Option<(u64, f32)> = None;
        for candidate_id in &combat_player_query.candidates {
            if *candidate_id == attacker_id {
                continue;
            }
            let Some(candidate) = players.get(candidate_id) else {
                continue;
            };
            let bounds = Aabb {
                min: [
                    candidate.position[0] - PLAYER_RADIUS,
                    candidate.position[1],
                    candidate.position[2] - PLAYER_RADIUS,
                ],
                max: [
                    candidate.position[0] + PLAYER_RADIUS,
                    candidate.position[1] + PLAYER_HEIGHT,
                    candidate.position[2] + PLAYER_RADIUS,
                ],
            };
            let Some(distance) = ray_aabb_distance(origin, direction, bounds, HITSCAN_RANGE) else {
                continue;
            };
            if nearest_player.is_none_or(|(nearest_id, nearest_distance)| {
                distance.total_cmp(&nearest_distance).is_lt()
                    || (distance == nearest_distance && *candidate_id < nearest_id)
            }) {
                nearest_player = Some((*candidate_id, distance));
            }
        }

        let nearest_blocker =
            combat_map_query.nearest_blocker(collision_index, origin, direction, HITSCAN_RANGE);
        if nearest_player.is_none_or(|(_, player_distance)| {
            nearest_blocker.is_some_and(|blocker_distance| blocker_distance <= player_distance)
        }) {
            return Some(ShotOutcome {
                target_id: None,
                remaining_health: None,
                killed: false,
            });
        }
        let (target_id, _) = nearest_player?;
        let target = players.get_mut(&target_id)?;
        let remaining_health = target.health.saturating_sub(HITSCAN_DAMAGE);
        let killed = remaining_health == 0;
        if killed {
            target.health = PLAYER_MAX_HEALTH;
            target.position = *spawn;
            target.velocity = [0.0; 3];
            target.grounded = false;
            update_cell_index(
                Arc::make_mut(cells),
                Arc::make_mut(player_cells),
                target_id,
                *spawn,
            );
        } else {
            target.health = remaining_health;
        }
        Some(ShotOutcome {
            target_id: Some(target_id),
            remaining_health: Some(remaining_health),
            killed,
        })
    }

    pub fn player_health(&self, id: u64) -> Option<u16> {
        self.players.get(&id).map(|player| player.health)
    }

    pub fn player(&self, id: u64) -> Option<&Player> {
        self.players.get(&id)
    }

    fn update_player_cell(&mut self, id: u64, position: [f32; 3]) {
        update_cell_index(
            Arc::make_mut(&mut self.cells),
            Arc::make_mut(&mut self.player_cells),
            id,
            position,
        );
    }

    pub fn rename_player(&mut self, id: u64, name: String) -> bool {
        let Some(player) = Arc::make_mut(&mut self.players).get_mut(&id) else {
            return false;
        };
        player.name = Arc::from(name.chars().take(24).collect::<String>());
        true
    }

    pub fn snapshot(&self) -> Vec<Player> {
        self.players.values().cloned().collect()
    }

    pub fn network_snapshot(&self) -> WorldSnapshot {
        WorldSnapshot {
            players: Arc::clone(&self.players),
            cells: Arc::clone(&self.cells),
        }
    }

    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    pub fn add_collider(&mut self, collider: Aabb) {
        Arc::make_mut(&mut self.collision_index).add(collider);
    }

    pub fn load_map(&mut self, map: &WorldMap) {
        self.spawn = map.spawn;
        self.collision_index = Arc::new(map.collision_index());
        self.collision_query = CollisionQuery::default();
        self.combat_player_query = PlayerRayQuery::default();
        self.combat_map_query = MapRayQuery::default();
        self.simulation_batches.clear();
    }
}

fn update_cell_index(
    cells: &mut HashMap<Cell, Vec<u64>>,
    player_cells: &mut HashMap<u64, Cell>,
    id: u64,
    position: [f32; 3],
) {
    let new_cell = cell_for(position);
    if player_cells.get(&id).copied() == Some(new_cell) {
        return;
    }
    let old_cell = player_cells.insert(id, new_cell);
    if old_cell != Some(new_cell) {
        if let Some(old_cell) = old_cell {
            let empty = if let Some(players) = cells.get_mut(&old_cell) {
                players.retain(|player| *player != id);
                players.is_empty()
            } else {
                false
            };
            if empty {
                cells.remove(&old_cell);
            }
        }
        cells.entry(new_cell).or_default().push(id);
    }
}

fn horizontal_overlap(position: [f32; 3], collider: Aabb) -> bool {
    position[0] - PLAYER_RADIUS < collider.max[0]
        && position[0] + PLAYER_RADIUS > collider.min[0]
        && position[2] - PLAYER_RADIUS < collider.max[2]
        && position[2] + PLAYER_RADIUS > collider.min[2]
}

fn intersects_player(position: [f32; 3], collider: Aabb) -> bool {
    let player_min = [
        position[0] - PLAYER_RADIUS,
        position[1],
        position[2] - PLAYER_RADIUS,
    ];
    let player_max = [
        position[0] + PLAYER_RADIUS,
        position[1] + PLAYER_HEIGHT,
        position[2] + PLAYER_RADIUS,
    ];
    (0..3)
        .all(|axis| player_min[axis] < collider.max[axis] && player_max[axis] > collider.min[axis])
}

fn cell_for(position: [f32; 3]) -> Cell {
    Cell(
        (position[0] / CELL_SIZE).floor() as i32,
        (position[1] / CELL_SIZE).floor() as i32,
        (position[2] / CELL_SIZE).floor() as i32,
    )
}

fn visit_ray_cells(
    origin: [f32; 3],
    direction: [f32; 3],
    maximum_distance: f32,
    mut visit: impl FnMut(Cell),
) {
    if !maximum_distance.is_finite()
        || maximum_distance <= 0.0
        || origin
            .iter()
            .chain(direction.iter())
            .any(|value| !value.is_finite())
    {
        return;
    }
    let start = cell_for(origin);
    let mut coordinates = [start.0, start.1, start.2];
    let mut step = [0i32; 3];
    let mut next_crossing = [f32::INFINITY; 3];
    let mut crossing_delta = [f32::INFINITY; 3];
    for axis in 0..3 {
        if direction[axis] > 0.0 {
            step[axis] = 1;
            let boundary = (coordinates[axis] + 1) as f32 * CELL_SIZE;
            next_crossing[axis] = (boundary - origin[axis]) / direction[axis];
            crossing_delta[axis] = CELL_SIZE / direction[axis];
        } else if direction[axis] < 0.0 {
            step[axis] = -1;
            let boundary = coordinates[axis] as f32 * CELL_SIZE;
            next_crossing[axis] = (boundary - origin[axis]) / direction[axis];
            crossing_delta[axis] = CELL_SIZE / -direction[axis];
        }
    }
    visit(Cell(coordinates[0], coordinates[1], coordinates[2]));
    for _ in 0..MAX_RAY_CELL_CROSSINGS {
        let crossing = next_crossing.iter().copied().fold(f32::INFINITY, f32::min);
        if !crossing.is_finite() || crossing > maximum_distance {
            break;
        }
        let mut tied_axes = 0u8;
        for (axis, value) in next_crossing.iter().enumerate() {
            if (*value - crossing).abs() <= 1.0e-5 {
                tied_axes |= 1 << axis;
            }
        }
        for subset in 1u8..8 {
            if subset & !tied_axes != 0 {
                continue;
            }
            let mut adjacent = coordinates;
            for axis in 0..3 {
                if subset & (1 << axis) != 0 {
                    adjacent[axis] += step[axis];
                }
            }
            visit(Cell(adjacent[0], adjacent[1], adjacent[2]));
        }
        for axis in 0..3 {
            if tied_axes & (1 << axis) != 0 {
                coordinates[axis] += step[axis];
                next_crossing[axis] += crossing_delta[axis];
            }
        }
    }
}

fn ray_aabb_distance(
    origin: [f32; 3],
    direction: [f32; 3],
    bounds: Aabb,
    maximum_distance: f32,
) -> Option<f32> {
    let mut near = 0.0f32;
    let mut far = maximum_distance;
    for axis in 0..3 {
        if direction[axis].abs() <= 1.0e-8 {
            if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] {
                return None;
            }
            continue;
        }
        let inverse = direction[axis].recip();
        let mut first = (bounds.min[axis] - origin[axis]) * inverse;
        let mut second = (bounds.max[axis] - origin[axis]) * inverse;
        if first > second {
            std::mem::swap(&mut first, &mut second);
        }
        near = near.max(first);
        far = far.min(second);
        if near > far {
            return None;
        }
    }
    (near <= maximum_distance).then_some(near)
}

fn ray_triangle_distance(
    origin: [f32; 3],
    direction: [f32; 3],
    triangle: MapTriangle,
    maximum_distance: f32,
) -> Option<f32> {
    let edge_one = subtract(triangle.vertices[1], triangle.vertices[0]);
    let edge_two = subtract(triangle.vertices[2], triangle.vertices[0]);
    let direction_cross_edge = cross(direction, edge_two);
    let determinant = dot(edge_one, direction_cross_edge);
    if determinant.abs() <= 1.0e-7 {
        return None;
    }
    let inverse_determinant = determinant.recip();
    let from_vertex = subtract(origin, triangle.vertices[0]);
    let u = dot(from_vertex, direction_cross_edge) * inverse_determinant;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(from_vertex, edge_one);
    let v = dot(direction, q) * inverse_determinant;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = dot(edge_two, q) * inverse_determinant;
    (distance >= 0.0 && distance <= maximum_distance).then_some(distance)
}

fn subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}
