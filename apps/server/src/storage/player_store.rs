use super::{
    format::{self, MAX_STORED_PLAYERS, PLAYER_STORE_SHARDS},
    invalid_world_snapshot, valid_player_name,
};
use minimium_core::ids::PlayerId;
use minimium_core::{
    identity::PlayerProfileId,
    world::{Player, World, WorldMap, PLAYER_MAX_HEALTH},
};
use minimium_protocol::MAX_PLAYER_NAME_BYTES;
use std::{
    collections::{HashMap, HashSet},
    io,
    path::Path,
    sync::Arc,
};

#[derive(Clone, Copy)]
pub(crate) struct StoredPlayer {
    pub(crate) id: PlayerId,
    pub(crate) name: StoredName,
    pub(crate) position: [f32; 3],
    pub(crate) health: u16,
}

#[derive(Clone, Copy)]
pub(crate) struct StoredName {
    bytes: [u8; MAX_PLAYER_NAME_BYTES],
    length: u8,
}

impl StoredName {
    pub(super) fn new(value: &str) -> Option<Self> {
        if !valid_player_name(value) {
            return None;
        }
        let mut bytes = [0; MAX_PLAYER_NAME_BYTES];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Some(Self {
            bytes,
            length: value.len() as u8,
        })
    }

    pub(crate) fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.length)]).unwrap_or("")
    }
}

pub(crate) struct PlayerStore {
    pub(super) players: Vec<Arc<HashMap<PlayerProfileId, StoredPlayer>>>,
    pub(super) profile_by_player: HashMap<PlayerId, PlayerProfileId>,
    active_players: HashSet<PlayerId>,
}

pub(crate) struct SaveSnapshot {
    pub(super) map_fingerprint: u64,
    pub(super) player_shards: Vec<Arc<HashMap<PlayerProfileId, StoredPlayer>>>,
}

impl Default for PlayerStore {
    fn default() -> Self {
        Self {
            players: (0..PLAYER_STORE_SHARDS)
                .map(|_| Arc::new(HashMap::new()))
                .collect(),
            profile_by_player: HashMap::new(),
            active_players: HashSet::new(),
        }
    }
}

fn player_store_shard(profile_id: PlayerProfileId) -> usize {
    usize::from(profile_id.as_bytes()[0])
}

impl PlayerStore {
    pub(crate) fn reserve_active(&mut self, capacity: usize) {
        self.active_players.reserve(capacity);
    }

    pub(crate) fn active_count(&self) -> usize {
        self.active_players.len()
    }

    pub(crate) fn is_active(&self, player: PlayerId) -> bool {
        self.active_players.contains(&player)
    }

    pub(crate) fn profile_for_player(&self, player: PlayerId) -> Option<PlayerProfileId> {
        self.profile_by_player.get(&player).copied()
    }
    pub(crate) fn load(data_dir: &Path, map: &WorldMap) -> io::Result<Self> {
        format::load(data_dir, map)
    }

    pub(crate) fn next_player_id(&self) -> io::Result<u64> {
        self.players
            .iter()
            .flat_map(|shard| shard.values())
            .map(|player| player.id.get())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| invalid_world_snapshot("player id space is exhausted"))
    }

    pub(crate) fn claim(
        &mut self,
        profile_id: PlayerProfileId,
        next_id: &mut u64,
        spawn: [f32; 3],
    ) -> io::Result<StoredPlayer> {
        let shard = player_store_shard(profile_id);
        if let Some(existing) = self.players[shard].get(&profile_id) {
            if self.active_players.contains(&existing.id) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "player profile is already connected",
                ));
            }
        } else {
            if self.profile_by_player.len() >= MAX_STORED_PLAYERS {
                return Err(io::Error::other(
                    "server has reached the saved player limit",
                ));
            }
            let id = *next_id;
            *next_id = id
                .checked_add(1)
                .ok_or_else(|| io::Error::other("player id space is exhausted"))?;
            let default_name = format!("Player{id}");
            let name = StoredName::new(&default_name)
                .or_else(|| StoredName::new(&format!("P{id}")))
                .ok_or_else(|| io::Error::other("could not create default player name"))?;
            let player = StoredPlayer {
                id: PlayerId::new(id),
                name,
                position: spawn,
                health: PLAYER_MAX_HEALTH,
            };
            Arc::make_mut(&mut self.players[shard]).insert(profile_id, player);
            self.profile_by_player.insert(PlayerId::new(id), profile_id);
        }
        let player = self.players[shard]
            .get(&profile_id)
            .copied()
            .ok_or_else(|| io::Error::other("player record disappeared"))?;
        if !self.active_players.insert(player.id) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "player profile is already connected",
            ));
        }
        Ok(player)
    }

    pub(crate) fn release(&mut self, player_id: PlayerId) {
        self.active_players.remove(&player_id);
    }

    pub(crate) fn update_from_player(&mut self, player: &Player) -> io::Result<()> {
        let profile_id = self
            .profile_by_player
            .get(&player.id)
            .copied()
            .ok_or_else(|| invalid_world_snapshot("active player has no profile id"))?;
        let shard = player_store_shard(profile_id);
        let stored = Arc::make_mut(&mut self.players[shard])
            .get_mut(&profile_id)
            .ok_or_else(|| invalid_world_snapshot("active player record is missing"))?;
        stored.name = StoredName::new(player.name.as_ref())
            .ok_or_else(|| invalid_world_snapshot("active player has an invalid name"))?;
        stored.position = player.position;
        stored.health = player.health;
        Ok(())
    }

    pub(crate) fn name_for_player(&self, player_id: PlayerId) -> Option<&str> {
        let profile_id = self.profile_by_player.get(&player_id)?;
        let shard = player_store_shard(*profile_id);
        self.players[shard]
            .get(profile_id)
            .map(|player| player.name.as_str())
    }

    pub(crate) fn rename(&mut self, player_id: PlayerId, name: String) {
        if let Some(profile_id) = self.profile_by_player.get(&player_id).copied() {
            let shard = player_store_shard(profile_id);
            if let Some(player) = Arc::make_mut(&mut self.players[shard]).get_mut(&profile_id) {
                if let Some(name) = StoredName::new(&name) {
                    player.name = name;
                }
            }
        }
    }

    pub(crate) fn save_snapshot(
        &mut self,
        world: &World,
        map_fingerprint: u64,
    ) -> io::Result<SaveSnapshot> {
        let _storage_phase =
            minimium_telemetry::allocations::enter(minimium_telemetry::allocations::Phase::Storage);
        for player in world.snapshot() {
            self.update_from_player(&player)?;
        }
        let player_shards = self.players.iter().map(Arc::clone).collect();
        Ok(SaveSnapshot {
            map_fingerprint,
            player_shards,
        })
    }
}

impl SaveSnapshot {
    pub(super) fn write(self, data_dir: &Path) -> io::Result<()> {
        format::write(self, data_dir)
    }
}
