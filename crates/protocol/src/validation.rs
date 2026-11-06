use crate::limits::{MAX_CHAT_BYTES, MAX_PLAYER_NAME_BYTES};

pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_PLAYER_NAME_BYTES
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

pub(crate) fn valid_chat(text: &str) -> bool {
    !text.is_empty() && text.len() <= MAX_CHAT_BYTES && !text.chars().any(char::is_control)
}

pub(crate) fn valid_direction_component(value: f32) -> bool {
    value.is_finite() && (-1.0..=1.0).contains(&value)
}

pub(crate) fn is_unit_direction(direction: [f32; 3]) -> bool {
    if direction.iter().any(|value| !value.is_finite()) {
        return false;
    }
    let length_squared = direction.iter().map(|value| value * value).sum::<f32>();
    (0.99..=1.01).contains(&length_squared)
}
