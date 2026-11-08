use crate::ProtocolError;

macro_rules! kinds {
    ($name:ident { $($variant:ident = $value:expr),+ $(,)? }) => {
        #[derive(Clone, Copy)]
        pub(crate) enum $name { $($variant = $value),+ }

        impl TryFrom<u8> for $name {
            type Error = ProtocolError;

            fn try_from(value: u8) -> Result<Self, ProtocolError> {
                match value {
                    $($value => Ok(Self::$variant),)+
                    other => Err(ProtocolError::UnknownMessage(other)),
                }
            }
        }

        impl From<$name> for u8 {
            fn from(kind: $name) -> Self { kind as u8 }
        }
    };
}

kinds!(ClientKind {
    Ping = 1, Quit = 4, Authenticate = 5,
    WorldInput = 8, TransitionReady = 9, CancelTransition = 11,
    TimeProbe = 0x0c,
    Presentation = 0x0d, ContentReady = 0x0e,
    ReplicationAck = 0x0f,
    SkyView = 0x10,
});
kinds!(ServerKind {
    Welcome = 0x81, Pong = 0x82, Accepted = 0x83, Bye = 0x84, Error = 0x85,
    Snapshot = 0x86, ShotResult = 0x87, HealthUpdate = 0x88, Chat = 0x89,
    TransitionPrepare = 0x8a, TransitionCommitted = 0x8b,
    TransitionCancelled = 0x8c, MapTeleportResult = 0x8d,
    EnvironmentSync = 0x8e, TimeProbeReply = 0x8f,
    ContentOffer = 0x90, AltitudeResult = 0x91,
    MovementPolicy = 0x92, EntitySyncBegin = 0x93,
    EntitySyncRecord = 0x94, EntitySyncComplete = 0x95,
    Bolid = 0x96,
});
