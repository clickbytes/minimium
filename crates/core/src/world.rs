use std::{
    collections::HashMap,
    fmt::Write as FmtWrite,
    fs::File,
    io::{self, Read},
    path::Path,
    sync::Arc,
};

const CELL_SIZE: f32 = 32.0;
const VISIBILITY_RADIUS: f32 = 72.0;
const PLAYER_RADIUS: f32 = 0.3;
const PLAYER_HEIGHT: f32 = 1.7;
const MAX_STEP_UP: f32 = 0.55;
const PLAYER_MOVE_SPEED: f32 = 5.0;
const PLAYER_JUMP_SPEED: f32 = 5.5;
const GRAVITY: f32 = 9.81;
const MAX_FALL_SPEED: f32 = 40.0;
const COLLISION_EPSILON: f32 = 0.0001;
const MAX_COLLIDER_CELLS: u64 = 64;
const MAX_QUERY_CELLS: u64 = 512;

#[derive(Clone, Debug)]
pub struct Player {
    pub id: u64,
    pub name: Arc<str>,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub grounded: bool,
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

#[derive(Default)]
pub struct CollisionIndex {
    colliders: Vec<Aabb>,
    cells: HashMap<Cell, Vec<usize>>,
    large_colliders: Vec<usize>,
    surfaces: Vec<MapTriangle>,
    surface_cells: HashMap<Cell, Vec<usize>>,
    large_surfaces: Vec<usize>,
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
        if !is_valid_solid_surface(surface) {
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

impl CollisionQuery {
    pub fn fill(&mut self, index: &CollisionIndex, state: CharacterState, delta_seconds: f32) {
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
            for candidate in 0..index.colliders.len() {
                self.mark_candidate(index, candidate, generation);
            }
            for candidate in 0..index.surfaces.len() {
                self.mark_surface_candidate(index, candidate, generation);
            }
            return;
        }

        let min_cell = cell_for(min);
        let max_cell = cell_for(max);
        if cell_volume(min_cell, max_cell) > MAX_QUERY_CELLS {
            for candidate in 0..index.colliders.len() {
                self.mark_candidate(index, candidate, generation);
            }
            for candidate in 0..index.surfaces.len() {
                self.mark_surface_candidate(index, candidate, generation);
            }
            return;
        }

        for z in min_cell.2..=max_cell.2 {
            for y in min_cell.1..=max_cell.1 {
                for x in min_cell.0..=max_cell.0 {
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
}

fn cell_volume(min: Cell, max: Cell) -> u64 {
    let width = (i64::from(max.0) - i64::from(min.0) + 1).max(0) as u64;
    let height = (i64::from(max.1) - i64::from(min.1) + 1).max(0) as u64;
    let depth = (i64::from(max.2) - i64::from(min.2) + 1).max(0) as u64;
    width.saturating_mul(height).saturating_mul(depth)
}

const MAX_MAP_BOXES: usize = 4096;
const MAX_MAP_TRIANGLES: usize = 8192;
const MAX_MAP_COORDINATE: f32 = 1_000_000.0;
const MAX_MAP_BYTES: usize = 1024 * 1024;

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
        Self::parse(&input).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn parse(source: &str) -> Result<Self, String> {
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
                    if solid && triangle_normal(vertices)[1].abs() < area_squared.sqrt() * 0.5 {
                        return Err(format!(
                            "line {line_number}: solid triangles must be floor or ceiling surfaces with slopes no steeper than 60 degrees"
                        ));
                    }
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
            .any(|triangle| intersects_triangle_surface(spawn, *triangle))
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
                .all(|triangle| !intersects_triangle_surface(position, *triangle))
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

fn is_valid_solid_surface(surface: MapTriangle) -> bool {
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
    normal_length_squared.is_finite()
        && normal_length_squared > 1.0e-12
        && normal[1].abs() >= normal_length_squared.sqrt() * 0.5
}

fn triangle_height_for_player(position: [f32; 3], surface: MapTriangle) -> Option<f32> {
    if !is_valid_solid_surface(surface) {
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
    player_cells: Arc<HashMap<u64, Cell>>,
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
        self.visit_nearby(recipient, limit, |player| {
            let _ = write!(
                output,
                " {} {} {:.2} {:.2} {:.2}",
                player.id, player.name, player.position[0], player.position[1], player.position[2]
            );
        });
    }

    pub fn visit_nearby(&self, recipient: u64, limit: usize, mut visit: impl FnMut(&Player)) {
        let Some(origin) = self.players.get(&recipient) else {
            return;
        };
        let cell = self
            .player_cells
            .get(&recipient)
            .copied()
            .unwrap_or_else(|| cell_for(origin.position));
        let mut visible_count = 0;
        let radius_squared = VISIBILITY_RADIUS * VISIBILITY_RADIUS;
        for z in -1..=1 {
            for y in -1..=1 {
                for x in -1..=1 {
                    let key = Cell(cell.0 + x, cell.1 + y, cell.2 + z);
                    let Some(ids) = self.cells.get(&key) else {
                        continue;
                    };
                    if ids.is_empty() {
                        continue;
                    }
                    let start = (recipient as usize) % ids.len();
                    for offset in 0..ids.len() {
                        if visible_count >= limit {
                            return;
                        }
                        let id = ids[(start + offset) % ids.len()];
                        if id == recipient {
                            continue;
                        }
                        if let Some(player) = self.players.get(&id) {
                            let distance_squared = (0..3)
                                .map(|axis| {
                                    let delta = player.position[axis] - origin.position[axis];
                                    delta * delta
                                })
                                .sum::<f32>();
                            if distance_squared <= radius_squared {
                                visit(player);
                                visible_count += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn nearby_players(&self, recipient: u64, limit: usize) -> Vec<Player> {
        let Some(origin) = self.players.get(&recipient) else {
            return Vec::new();
        };
        let cell = self
            .player_cells
            .get(&recipient)
            .copied()
            .unwrap_or_else(|| cell_for(origin.position));
        let radius_squared = VISIBILITY_RADIUS * VISIBILITY_RADIUS;
        let mut visible = Vec::with_capacity(limit.min(16));
        for z in -1..=1 {
            for y in -1..=1 {
                for x in -1..=1 {
                    let key = Cell(cell.0 + x, cell.1 + y, cell.2 + z);
                    let Some(ids) = self.cells.get(&key) else {
                        continue;
                    };
                    if ids.is_empty() {
                        continue;
                    }
                    let start = (recipient as usize) % ids.len();
                    for offset in 0..ids.len() {
                        if visible.len() >= limit {
                            return visible;
                        }
                        let id = ids[(start + offset) % ids.len()];
                        if id == recipient {
                            continue;
                        }
                        if let Some(player) = self.players.get(&id) {
                            let distance_squared = (0..3)
                                .map(|axis| {
                                    let delta = player.position[axis] - origin.position[axis];
                                    delta * delta
                                })
                                .sum::<f32>();
                            if distance_squared <= radius_squared {
                                visible.push(player.clone());
                            }
                        }
                    }
                }
            }
        }
        visible
    }
}

#[derive(Default)]
pub struct World {
    players: Arc<HashMap<u64, Player>>,
    cells: Arc<HashMap<Cell, Vec<u64>>>,
    player_cells: Arc<HashMap<u64, Cell>>,
    collision_index: CollisionIndex,
    collision_query: CollisionQuery,
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

    pub fn step_players(&mut self, inputs: &HashMap<u64, CharacterInput>, delta_seconds: f32) {
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
        for (id, player) in players {
            let mut state = CharacterState {
                position: player.position,
                velocity: player.velocity,
                grounded: player.grounded,
            };
            let input = inputs.get(id).copied().unwrap_or_default();
            collision_query.fill(collision_index, state, delta_seconds);
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
            player_cells: Arc::clone(&self.player_cells),
        }
    }

    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    pub fn add_collider(&mut self, collider: Aabb) {
        self.collision_index.add(collider);
    }

    pub fn load_map(&mut self, map: &WorldMap) {
        self.spawn = map.spawn;
        self.collision_index = map.collision_index();
        self.collision_query = CollisionQuery::default();
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
