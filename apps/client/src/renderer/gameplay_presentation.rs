use super::{GameplayPresentation, REMOTE_INTERPOLATION_DELAY, Vertex, cube};
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
            let position: [f32; 3] = std::array::from_fn(|axis| {
                (position_mm[axis] as f64 / 1000.0 - f64::from(origin[axis])) as f32
            });
            cube(
                &mut self.remote_vertices,
                [position[0], position[1] + 0.85, position[2]],
                [0.6, 1.7, 0.6],
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
            let feet = track
                .displayed_position_mm
                .map(|value| value as f64 / 1000.0);
            let shape_center = [feet[0], feet[1] + 0.85, feet[2]];
            let half = [0.3, 0.85, 0.3];
            let distance_squared = (0..3)
                .map(|axis| {
                    let delta = (center[axis] - shape_center[axis]).abs() - half[axis];
                    delta.max(0.0).powi(2)
                })
                .sum::<f64>();
            if distance_squared > 256.0_f64.powi(2) {
                continue;
            }
            let relative: [f32; 3] =
                std::array::from_fn(|axis| (feet[axis] - f64::from(origin[axis])) as f32);
            cube(
                &mut shadow_vertices,
                [relative[0], relative[1] + 0.85, relative[2]],
                [0.6, 1.7, 0.6],
                [0.82, 0.16, 0.12],
            );
        }
        let changed = bytemuck::cast_slice::<Vertex, u8>(&shadow_vertices)
            != bytemuck::cast_slice::<Vertex, u8>(&self.remote_shadow_vertices);
        if changed {
            self.remote_shadow_vertices = shadow_vertices;
        }
        changed
    }
}
