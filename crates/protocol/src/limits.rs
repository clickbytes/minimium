pub const PROTOCOL_VERSION: u8 = 8;
pub const MAX_PLAYER_NAME_BYTES: usize = 24;
pub const HEADER_BYTES: usize = 4;
pub const MAX_PAYLOAD_BYTES: usize = 4096;
pub const MAX_FRAME_BYTES: usize = HEADER_BYTES + MAX_PAYLOAD_BYTES;
pub const MAX_VISIBLE_PLAYERS: usize = 16;
pub const MAX_SERVER_FRAME_BYTES: usize =
    HEADER_BYTES + 36 + MAX_VISIBLE_PLAYERS * (45 + std::mem::size_of::<u64>());
pub const PLAYER_PROFILE_ID_BYTES: usize = 16;
pub const MAX_CHAT_BYTES: usize = 128;

pub(crate) const CLIENT_PING: u8 = 1;
pub(crate) const CLIENT_SET_NAME: u8 = 2;
pub(crate) const CLIENT_MOVE: u8 = 3;
pub(crate) const CLIENT_QUIT: u8 = 4;
pub(crate) const CLIENT_IDENTIFY: u8 = 5;
pub(crate) const CLIENT_FIRE: u8 = 6;
pub(crate) const CLIENT_CHAT: u8 = 7;
pub(crate) const SERVER_WELCOME: u8 = 0x81;
pub(crate) const SERVER_PONG: u8 = 0x82;
pub(crate) const SERVER_ACCEPTED: u8 = 0x83;
pub(crate) const SERVER_BYE: u8 = 0x84;
pub(crate) const SERVER_ERROR: u8 = 0x85;
pub(crate) const SERVER_SNAPSHOT: u8 = 0x86;
pub(crate) const SERVER_SHOT_RESULT: u8 = 0x87;
pub(crate) const SERVER_HEALTH_UPDATE: u8 = 0x88;
pub(crate) const SERVER_CHAT: u8 = 0x89;

pub const ERROR_PROTOCOL: u8 = 1;
pub const ERROR_BUSY: u8 = 2;
pub const ERROR_SERVER_FULL: u8 = 3;
pub const ERROR_RATE_LIMIT: u8 = 4;
