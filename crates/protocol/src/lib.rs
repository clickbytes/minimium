mod decoding;
mod encoding;
mod environment_codec;
mod error_code;
mod framing;
mod kinds;
mod limits;
mod models;
mod reading;
mod validation;
mod world_context;

#[cfg(test)]
mod tests;

pub use decoding::{decode_client, decode_server};
pub use encoding::{encode_client, encode_server, encode_server_into};
pub use error_code::ServerErrorCode;
pub use limits::{
    HEADER_BYTES, MAX_CHAT_BYTES, MAX_FRAME_BYTES, MAX_PAYLOAD_BYTES, MAX_PLAYER_NAME_BYTES,
    MAX_SERVER_FRAME_BYTES, MAX_VISIBLE_PLAYERS, PLAYER_PROFILE_ID_BYTES, PROTOCOL_VERSION,
};
pub use models::{
    ClientMessage, ClientWorldInput, ProtocolError, RemotePlayer, RemotePlayerIds, RemotePlayers,
    ServerMessage,
};
pub use world_context::WorldContext;
