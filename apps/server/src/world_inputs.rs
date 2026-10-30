use minimium_core::{
    ids::PlayerId,
    world::{CharacterInput, ShotOutcome},
};

pub(crate) enum WorldInput {
    Join(PlayerId, String, [f32; 3], u16),
    Leave(PlayerId),
    Move(PlayerId, CharacterInput),
    Fire(PlayerId, [f32; 3]),
    Chat(PlayerId, String),
    Rename(PlayerId, String),
}

pub(crate) enum WorldEvent {
    Shot {
        shooter: PlayerId,
        outcome: ShotOutcome,
        target_health: Option<u16>,
    },
    Chat {
        sender: PlayerId,
        name: String,
        text: String,
    },
}
