//! Bounded network snapshot reconciliation and remote-player interpolation.
use minimium_core::ids::PlayerId;
use minimium_protocol::{RemotePlayerIds, RemotePlayers, MAX_VISIBLE_PLAYERS};
use std::time::{Duration, Instant};

const TELEPORT_THRESHOLD_MM: f64 = 100_000.0;

#[derive(Clone, Copy)]
pub struct RemotePlayerTrack {
    pub id: PlayerId,
    pub previous_position_mm: [i64; 3],
    pub current_position_mm: [i64; 3],
    pub displayed_position_mm: [i64; 3],
    previous_time_ms: u64,
    current_time_ms: u64,
    displayed_time_ms: u64,
}

#[derive(Default)]
pub struct SnapshotReconciler {
    tracks: [Option<RemotePlayerTrack>; MAX_VISIBLE_PLAYERS],
    last_server_time_ms: Option<u64>,
    last_received_at: Option<Instant>,
    geometry_dirty: bool,
}

impl SnapshotReconciler {
    pub fn apply_remote_snapshot(
        &mut self,
        server_time_ms: u64,
        full_snapshot: bool,
        players: &RemotePlayers,
        removed_players: &RemotePlayerIds,
    ) -> bool {
        let server_time_ms = self
            .last_server_time_ms
            .map_or(server_time_ms, |previous| previous.max(server_time_ms));
        self.last_server_time_ms = Some(server_time_ms);
        self.last_received_at = Some(Instant::now());
        if full_snapshot {
            self.tracks = [None; MAX_VISIBLE_PLAYERS];
        }
        for id in removed_players.iter() {
            if let Some(slot) = self
                .tracks
                .iter_mut()
                .find(|slot| slot.is_some_and(|track| track.id == *id))
            {
                *slot = None;
            }
        }
        for player in players.iter() {
            let existing_slot = self
                .tracks
                .iter()
                .position(|slot| slot.is_some_and(|track| track.id == player.id));
            let slot = existing_slot.or_else(|| self.tracks.iter().position(Option::is_none));
            let Some(slot) = slot else {
                return false;
            };
            let existing = self.tracks[slot];
            let mut previous_position_mm = existing
                .map(|track| track.current_position_mm)
                .unwrap_or(player.position_mm.0);
            let displacement_squared = previous_position_mm
                .iter()
                .zip(player.position_mm.0)
                .map(|(previous, current)| (*previous as f64 - current as f64).powi(2))
                .sum::<f64>();
            let teleported = displacement_squared > TELEPORT_THRESHOLD_MM.powi(2);
            if teleported {
                previous_position_mm = player.position_mm.0;
            }
            let mut previous_time_ms = existing
                .map(|track| track.current_time_ms.min(server_time_ms))
                .unwrap_or(server_time_ms);
            if teleported {
                previous_time_ms = server_time_ms;
            }
            self.tracks[slot] = Some(RemotePlayerTrack {
                id: player.id,
                previous_position_mm,
                current_position_mm: player.position_mm.0,
                displayed_position_mm: previous_position_mm,
                previous_time_ms,
                current_time_ms: server_time_ms,
                displayed_time_ms: previous_time_ms,
            });
        }
        self.geometry_dirty = true;
        true
    }

    pub fn clear(&mut self) {
        self.tracks = [None; MAX_VISIBLE_PLAYERS];
        self.last_server_time_ms = None;
        self.last_received_at = None;
        self.geometry_dirty = false;
    }

    pub fn player_count(&self) -> usize {
        self.tracks.iter().flatten().count()
    }

    pub fn tracks(&self) -> &[Option<RemotePlayerTrack>; MAX_VISIBLE_PLAYERS] {
        &self.tracks
    }

    /// Updates and writes displayed world-space millimetre positions into a reusable host buffer.
    pub fn interpolate_positions(
        &mut self,
        now: Instant,
        delay: Duration,
        output: &mut Vec<[i64; 3]>,
    ) -> bool {
        output.clear();
        let estimated_server_time_ms = match (self.last_server_time_ms, self.last_received_at) {
            (Some(server_time_ms), Some(received_at)) => {
                let elapsed = u64::try_from(now.saturating_duration_since(received_at).as_millis())
                    .unwrap_or(u64::MAX);
                server_time_ms.saturating_add(elapsed)
            }
            _ => 0,
        };
        let render_time_ms = estimated_server_time_ms
            .saturating_sub(u64::try_from(delay.as_millis()).unwrap_or(u64::MAX));
        let mut moved = false;
        for track in self.tracks.iter_mut().flatten() {
            let interval_ms = track.current_time_ms.saturating_sub(track.previous_time_ms);
            let interpolation = if interval_ms == 0 {
                1.0
            } else {
                (render_time_ms.saturating_sub(track.previous_time_ms) as f64 / interval_ms as f64)
                    .clamp(0.0, 1.0)
            };
            let position_mm = std::array::from_fn(|axis| {
                (track.previous_position_mm[axis] as f64 * (1.0 - interpolation)
                    + track.current_position_mm[axis] as f64 * interpolation)
                    .round() as i64
            });
            if position_mm != track.displayed_position_mm {
                track.displayed_position_mm = position_mm;
                moved = true;
            }
            track.displayed_time_ms = render_time_ms.clamp(
                track.previous_time_ms,
                track.current_time_ms.max(track.previous_time_ms),
            );
            output.push(track.displayed_position_mm);
        }
        let dirty = std::mem::take(&mut self.geometry_dirty);
        moved || dirty
    }
}
