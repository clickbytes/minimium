pub const PROTOCOL_VERSION: u8 = 11;
pub const MAX_PLAYER_NAME_BYTES: usize = 24;
pub const HEADER_BYTES: usize = 4;
pub const MAX_PAYLOAD_BYTES: usize = 4096;
pub const MAX_FRAME_BYTES: usize = HEADER_BYTES + MAX_PAYLOAD_BYTES;
pub const MAX_VISIBLE_PLAYERS: usize = 16;
pub const MAX_SERVER_FRAME_BYTES: usize =
    HEADER_BYTES + 36 + MAX_VISIBLE_PLAYERS * (45 + std::mem::size_of::<u64>());
pub use minimium_core::identity::PLAYER_PROFILE_ID_BYTES;
pub const MAX_CHAT_BYTES: usize = 128;
