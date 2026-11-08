use minimium_core::ids::PlayerId;
use minimium_protocol::WorldContext;
use minimium_simulation::ShotOutcome;
use minimium_world::world::{Position3Mm, WorldInstanceId};

pub(crate) enum WorldInput {
    Join {
        id: PlayerId,
        instance: WorldInstanceId,
        name: String,
        position: [f32; 3],
        position_mm: Position3Mm,
        health: u16,
    },
    Leave(PlayerId),
    Move(PlayerId, minimium_protocol::MovementSample),
    Fire(PlayerId, [f32; 3]),
    MapTeleport(PlayerId, WorldContext, [f32; 3]),
    SetAltitude(PlayerId, WorldContext, f32),
    Chat(PlayerId, String),
    Rename(PlayerId, String),
    InteractEntity(PlayerId, WorldContext, minimium_world::entity::EntityId),
}

pub(crate) enum WorldEvent {
    Shot {
        instance: WorldInstanceId,
        shooter: PlayerId,
        outcome: ShotOutcome,
        target_health: Option<u16>,
    },
    Chat {
        instance: WorldInstanceId,
        sender: PlayerId,
        name: String,
        text: String,
    },
}
