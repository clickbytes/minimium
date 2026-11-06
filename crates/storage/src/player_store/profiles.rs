use super::{PlayerStore, StoredName, StoredPlayer};
use crate::{format, invalid_world_snapshot};
use minimium_core::identity::PlayerProfileId;
use minimium_core::ids::PlayerId;
use minimium_simulation::{Player, PLAYER_MAX_HEALTH};
use minimium_world::world::{CharacterGravity, WorldInstanceId, WorldLocation};
use std::{io, sync::Arc};

fn player_store_shard(profile_id: PlayerProfileId) -> usize {
    usize::from(profile_id.as_bytes()[0])
}

impl PlayerStore {
    pub fn reserve_active(&mut self, capacity: usize) {
        self.active_players.reserve(capacity);
    }

    pub fn active_count(&self) -> usize {
        self.active_players.len()
    }

    pub fn is_active(&self, player: PlayerId) -> bool {
        self.active_players.contains(&player)
    }

    pub fn profile_for_player(&self, player: PlayerId) -> Option<PlayerProfileId> {
        self.profile_by_player.get(&player).copied()
    }

    pub fn next_player_id(&self) -> io::Result<u64> {
        self.players
            .iter()
            .flat_map(|shard| shard.values())
            .map(|player| player.id.get())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| invalid_world_snapshot("player id space is exhausted"))
    }

    pub fn claim(
        &mut self,
        profile_id: PlayerProfileId,
        next_id: &mut u64,
        spawn: [f32; 3],
        spawn_mm: Option<minimium_world::world::Position3Mm>,
        spawn_instance: WorldInstanceId,
        spawn_gravity: CharacterGravity,
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
            if self.profile_by_player.len() >= format::MAX_STORED_PLAYERS {
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
            let spawn_location =
                crate::world_location::world_location(spawn_instance, spawn, spawn_gravity);
            let spawn_location = spawn_mm.map_or(spawn_location, |position| {
                WorldLocation::new(spawn_instance, position, spawn_location.orientation())
            });
            let player = StoredPlayer {
                id: PlayerId::new(id),
                name,
                position: spawn,
                health: PLAYER_MAX_HEALTH,
                location: spawn_location,
                return_location: spawn_location,
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

    pub fn release(&mut self, player_id: PlayerId) {
        self.active_players.remove(&player_id);
    }

    pub fn update_from_player(
        &mut self,
        player: &Player,
        instance: WorldInstanceId,
        gravity: CharacterGravity,
    ) -> io::Result<()> {
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
        stored.location = crate::world_location::player_location(instance, player, gravity);
        Ok(())
    }

    pub fn set_return_location(&mut self, player: PlayerId, location: WorldLocation) {
        let Some(profile_id) = self.profile_by_player.get(&player).copied() else {
            return;
        };
        let shard = player_store_shard(profile_id);
        if let Some(stored) = Arc::make_mut(&mut self.players[shard]).get_mut(&profile_id) {
            stored.return_location = location;
        }
    }

    pub fn return_location_for(&self, player: PlayerId) -> Option<WorldLocation> {
        let profile_id = self.profile_by_player.get(&player)?;
        let shard = player_store_shard(*profile_id);
        self.players[shard]
            .get(profile_id)
            .map(|stored| stored.return_location)
    }

    pub fn name_for_player(&self, player_id: PlayerId) -> Option<&str> {
        let profile_id = self.profile_by_player.get(&player_id)?;
        let shard = player_store_shard(*profile_id);
        self.players[shard]
            .get(profile_id)
            .map(|player| player.name.as_str())
    }

    pub fn rename(&mut self, player_id: PlayerId, name: String) {
        if let Some(profile_id) = self.profile_by_player.get(&player_id).copied() {
            let shard = player_store_shard(profile_id);
            if let Some(player) = Arc::make_mut(&mut self.players[shard]).get_mut(&profile_id) {
                if let Some(name) = StoredName::new(&name) {
                    player.name = name;
                }
            }
        }
    }
}
