use minimium_core::{
    ids::PlayerId,
    world::{CharacterInput, Position3Mm, ShotOutcome, WorldInstanceId},
};
use minimium_protocol::WorldContext;

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
    Move(PlayerId, CharacterInput),
    Fire(PlayerId, [f32; 3]),
    MapTeleport(PlayerId, WorldContext, [f32; 3]),
    Chat(PlayerId, String),
    Rename(PlayerId, String),
    UsePortal(PlayerId, WorldContext),
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
