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
    Ping = 1, SetName = 2, Move = 3, Quit = 4, Identify = 5, Fire = 6, Chat = 7,
    WorldInput = 8, TransitionReady = 9, UsePortal = 10, CancelTransition = 11,
});
kinds!(ServerKind {
    Welcome = 0x81, Pong = 0x82, Accepted = 0x83, Bye = 0x84, Error = 0x85,
    Snapshot = 0x86, ShotResult = 0x87, HealthUpdate = 0x88, Chat = 0x89,
    TransitionPrepare = 0x8a, TransitionCommitted = 0x8b,
    TransitionCancelled = 0x8c,
});
