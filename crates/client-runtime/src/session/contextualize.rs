use super::QueueError;
use minimium_protocol::{ClientMessage, ClientWorldInput, WorldContext};

pub(super) fn contextualize(
    message: &ClientMessage,
    current_context: Option<WorldContext>,
) -> Result<Option<ClientMessage>, QueueError> {
    let contextual = match message {
        ClientMessage::Motion(input) => Some(ClientMessage::WorldInput {
            context: current_context.ok_or(QueueError::AwaitingWelcome)?,
            input: ClientWorldInput::Motion(*input),
        }),
        ClientMessage::SetName(name) => Some(ClientMessage::WorldInput {
            context: current_context.ok_or(QueueError::AwaitingWelcome)?,
            input: ClientWorldInput::SetName(name.clone()),
        }),
        ClientMessage::Move { direction, jump } => Some(ClientMessage::WorldInput {
            context: current_context.ok_or(QueueError::AwaitingWelcome)?,
            input: ClientWorldInput::Move {
                direction: *direction,
                jump: *jump,
            },
        }),
        ClientMessage::Fire { direction } => Some(ClientMessage::WorldInput {
            context: current_context.ok_or(QueueError::AwaitingWelcome)?,
            input: ClientWorldInput::Fire {
                direction: *direction,
            },
        }),
        ClientMessage::SetAltitude { km } => Some(ClientMessage::WorldInput {
            context: current_context.ok_or(QueueError::AwaitingWelcome)?,
            input: ClientWorldInput::SetAltitude { km: *km },
        }),
        ClientMessage::MapTeleport { direction } => Some(ClientMessage::WorldInput {
            context: current_context.ok_or(QueueError::AwaitingWelcome)?,
            input: ClientWorldInput::MapTeleport {
                direction: *direction,
            },
        }),
        ClientMessage::Chat(text) => Some(ClientMessage::WorldInput {
            context: current_context.ok_or(QueueError::AwaitingWelcome)?,
            input: ClientWorldInput::Chat(text.clone()),
        }),
        ClientMessage::UsePortal => Some(ClientMessage::WorldInput {
            context: current_context.ok_or(QueueError::AwaitingWelcome)?,
            input: ClientWorldInput::UsePortal,
        }),
        ClientMessage::WorldInput { context, .. } if Some(*context) != current_context => {
            return Err(QueueError::StaleWorldContext);
        }
        _ => None,
    };
    Ok(contextual)
}
