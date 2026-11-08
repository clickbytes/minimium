pub const PROTOCOL_VERSION: u8 = 19;
pub use minimium_core::identity::MAX_PLAYER_NAME_BYTES;
pub const HEADER_BYTES: usize = 4;
pub const MAX_PAYLOAD_BYTES: usize = 4096;
pub const MAX_FRAME_BYTES: usize = HEADER_BYTES + MAX_PAYLOAD_BYTES;
pub const MAX_VISIBLE_PLAYERS: usize = 16;
// Snapshot: uptime(8) + context(32) + precise position(24) + velocity(12)
// + flags/counts(4), followed by max-sized updated and removed player lists.
pub const MAX_SERVER_FRAME_BYTES: usize = MAX_FRAME_BYTES;
pub use minimium_core::identity::PLAYER_PROFILE_ID_BYTES;
pub const MAX_CHAT_BYTES: usize = 128;
pub const MAX_AUTH_PROVIDER_BYTES: usize = 32;
pub const MAX_AUTH_CREDENTIAL_BYTES: usize = 2048;
pub const MAX_COMPONENT_BYTES: usize = minimium_world::entity::MAX_COMPONENT_BYTES;
