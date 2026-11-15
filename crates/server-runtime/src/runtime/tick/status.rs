use super::super::ServerRuntime;
use crate::commands::{ClientTrafficRow, ClientTrafficSnapshot, RuntimeStatusSnapshot};
use minimium_protocol::PROTOCOL_VERSION;
use std::net::SocketAddr;

impl ServerRuntime {
    pub(super) fn runtime_status_snapshot(
        &self,
        game_address: SocketAddr,
        max_players: usize,
    ) -> RuntimeStatusSnapshot {
        let (environment_profiles, environment_profiles_truncated) =
            self.game.worlds.environment_snapshot(128);
        RuntimeStatusSnapshot {
            game_address: game_address.to_string(),
            max_players,
            map_fingerprint: self.map.fingerprint(),
            tick_ms: super::super::TICK.as_millis(),
            snapshot_ms: super::super::SNAPSHOT_INTERVAL.as_millis(),
            autosave_seconds: self.autosave_seconds,
            protocol_version: u32::from(PROTOCOL_VERSION),
            save_version: u32::from(super::super::SAVE_VERSION),
            environment_profiles,
            environment_profiles_truncated,
            impact: self.impact_status_snapshot(),
            excavation: self.excavation_status_snapshot(),
        }
    }

    pub(super) fn client_traffic_snapshot(&self) -> ClientTrafficSnapshot {
        let client_count = self
            .connections
            .clients
            .iter()
            .filter(|client| client.player_id().is_some())
            .count();
        let rows = self
            .connections
            .clients
            .iter()
            .filter_map(|client| {
                client.player_id().map(|id| ClientTrafficRow {
                    id,
                    bytes_in: client.bytes_in,
                    bytes_out: client.bytes_out,
                    decoded_frames: client.frames_in,
                    queued_frames: client.frames_out,
                    encoding_us: client.encoding_us,
                    connected_ms: client.connected_at.elapsed().as_millis(),
                })
            })
            .take(512)
            .collect();
        ClientTrafficSnapshot {
            rows,
            truncated: client_count > 512,
        }
    }
}
