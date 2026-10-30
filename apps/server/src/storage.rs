mod format;
mod loaded_state;
mod persistence;
mod player_store;

pub(crate) use format::SAVE_VERSION;
pub(crate) use loaded_state::LoadedState;
pub(crate) use persistence::PersistenceWorker;
pub(crate) use player_store::PlayerStore;

pub(crate) fn compact_error(error: &std::io::Error) -> String {
    error
        .to_string()
        .chars()
        .take(120)
        .map(|character| {
            if character.is_ascii_whitespace() {
                '_'
            } else {
                character
            }
        })
        .collect()
}

pub(super) fn valid_player_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= minimium_protocol::MAX_PLAYER_NAME_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

pub(super) fn invalid_world_snapshot(message: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}
