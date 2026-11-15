use crate::runtime::ServerRuntime;
use minimium_core::identity::PlayerProfileId;
use minimium_core::ids::PlayerId;
use minimium_world::world::{Position3Mm, WorldLocation};
use std::sync::Arc;

pub(super) struct AdmittedPlayer {
    pub id: PlayerId,
    pub name: String,
    pub position: [f32; 3],
    pub health: u16,
    pub location: WorldLocation,
}

pub(super) fn claim_and_validate(
    runtime: &mut ServerRuntime,
    profile: PlayerProfileId,
) -> Result<AdmittedPlayer, ()> {
    let primary_instance = runtime.game.worlds.primary_id();
    let primary = runtime.game.worlds.primary();
    let spawn_mm = if primary.manifest.planet().is_some() {
        super::super::gameplay_terrain::safe_planet_spawn(
            &runtime.game.worlds,
            primary_instance,
            runtime.prepared_geography.as_ref(),
            runtime.excavation.soil_depth_m(),
        )
        .ok_or(())?
    } else {
        primary.map.spawn_position_mm().unwrap_or_else(|| {
            Position3Mm(
                primary
                    .map
                    .spawn()
                    .map(|value| (f64::from(value) * 1000.0).round() as i64),
            )
        })
    };
    let spawn = spawn_mm.0.map(|value| value as f32 / 1000.0);
    let player = runtime
        .operations
        .player_store
        .claim(
            profile,
            &mut runtime.operations.next_player,
            spawn,
            Some(spawn_mm),
            primary_instance,
            primary.world.gravity(),
        )
        .map_err(|error| {
            crate::diagnostics::emit(
                &runtime.world_logger,
                minimium_diagnostics::Level::Warn,
                "server.profile_claim_failed",
                &[("reason", error.to_string())],
            );
        })?;
    let terrain_reads = super::super::gameplay_terrain::build(
        &runtime.game.worlds,
        runtime.prepared_geography.as_ref(),
        runtime.excavation.soil_depth_m(),
    );
    let saved_position = player.location.position();
    let saved_float = saved_position.0.map(|value| value as f32 / 1000.0);
    let saved_clear = runtime
        .game
        .worlds
        .get(player.location.instance())
        .is_some_and(|owner| {
            owner
                .map
                .can_place_player_with_gravity(saved_float, owner.world.gravity())
                && (owner.manifest.planet().is_none()
                    || terrain_reads
                        .get(&player.location.instance())
                        .is_some_and(|read| {
                            crate::world_registry::terrain_capsule_clear(read, saved_position)
                        }))
        });
    if saved_clear {
        return Ok(AdmittedPlayer {
            id: player.id,
            name: player.name.as_str().to_owned(),
            position: player.position,
            health: player.health,
            location: player.location,
        });
    }

    let owner = runtime.game.worlds.primary();
    let mut repaired = minimium_simulation::Player {
        id: player.id,
        name: Arc::from(player.name.as_str()),
        position: spawn,
        position_mm: Some(spawn_mm),
        velocity: [0.0; 3],
        grounded: false,
        health: player.health,
    };
    let location = minimium_storage::world_location::player_location(
        primary_instance,
        &repaired,
        owner.world.gravity(),
    );
    repaired.position_mm = Some(location.position());
    if let Err(error) = runtime.operations.player_store.update_from_player(
        &repaired,
        primary_instance,
        owner.world.gravity(),
    ) {
        crate::diagnostics::emit(
            &runtime.world_logger,
            minimium_diagnostics::Level::Error,
            "server.player_placement_repair_failed",
            &[
                ("player_id", player.id.to_string()),
                ("reason", error.to_string()),
            ],
        );
    }
    runtime
        .operations
        .player_store
        .set_return_location(player.id, location);
    let mut player = player;
    player.position = repaired.position;
    player.location = location;
    Ok(AdmittedPlayer {
        id: player.id,
        name: player.name.as_str().to_owned(),
        position: player.position,
        health: player.health,
        location: player.location,
    })
}
