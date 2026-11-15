use super::{cube, CharacterGeometry, GameplayPresentation, Vertex, REMOTE_INTERPOLATION_DELAY};
use crate::application::GameplayState;
use std::time::Instant;

impl GameplayPresentation {
    pub(super) fn clear_remote_players(&mut self) {
        self.remote_vertices.clear();
        self.remote_shadow_vertices.clear();
        self.remote_geometry_dirty = false;
        self.last_shot_sent_at = None;
        self.hit_confirmation_until = None;
    }

    pub(super) fn update_remote_vertices(
        &mut self,
        gameplay: &mut GameplayState,
        now: Instant,
        origin: [f32; 3],
    ) -> bool {
        let moved = gameplay.reconciliation.interpolate_positions(
            now,
            REMOTE_INTERPOLATION_DELAY,
            &mut gameplay.remote_positions_mm,
        );
        if !moved && !self.remote_geometry_dirty {
            return false;
        }
        self.remote_vertices.clear();
        for position_mm in &gameplay.remote_positions_mm {
            let feet_m = position_mm.map(|value| value as f64 / 1000.0);
            let Some(geometry) =
                CharacterGeometry::new(feet_m, gameplay.prediction.character.gravity)
            else {
                continue;
            };
            append_character_mesh(
                &mut self.remote_vertices,
                geometry,
                origin,
                [0.82, 0.16, 0.12],
            );
        }
        self.remote_geometry_dirty = false;
        true
    }

    pub(super) fn update_remote_shadow_vertices(
        &mut self,
        gameplay: &GameplayState,
        center: [f64; 3],
        origin: [f32; 3],
    ) -> bool {
        let mut shadow_vertices = Vec::with_capacity(gameplay.reconciliation.player_count() * 36);
        for track in gameplay.reconciliation.tracks().iter().flatten() {
            let feet_m = track
                .displayed_position_mm
                .map(|value| value as f64 / 1000.0);
            let Some(geometry) =
                CharacterGeometry::new(feet_m, gameplay.prediction.character.gravity)
            else {
                continue;
            };
            let shape_center = geometry.body_center_m();
            let caster_radius = geometry.body_bounding_radius_m();
            let distance_squared = (0..3)
                .map(|axis| (center[axis] - shape_center[axis]).powi(2))
                .sum::<f64>();
            if distance_squared > (256.0 + caster_radius).powi(2) {
                continue;
            }
            append_character_mesh(&mut shadow_vertices, geometry, origin, [0.82, 0.16, 0.12]);
        }
        let changed = bytemuck::cast_slice::<Vertex, u8>(&shadow_vertices)
            != bytemuck::cast_slice::<Vertex, u8>(&self.remote_shadow_vertices);
        if changed {
            self.remote_shadow_vertices = shadow_vertices;
        }
        changed
    }
}

fn append_character_mesh(
    output: &mut Vec<Vertex>,
    geometry: CharacterGeometry,
    origin: [f32; 3],
    color: [f32; 3],
) {
    let first = output.len();
    let size = geometry.body_size_m();
    let normal_sign = geometry.frame_handedness();
    cube(output, [0.0, geometry.height_m() * 0.5, 0.0], size, color);
    for vertex in &mut output[first..] {
        let world = geometry.local_to_world_point(vertex.position.map(f64::from));
        vertex.position =
            std::array::from_fn(|axis| (world[axis] - f64::from(origin[axis])) as f32);
        let normal = geometry.local_to_world_direction(vertex.normal.map(f64::from));
        let length = normal.iter().map(|value| value * value).sum::<f64>().sqrt();
        if length.is_finite() && length > 1.0e-9 {
            vertex.normal = normal.map(|value| (value / length * normal_sign) as f32);
        }
    }
}
