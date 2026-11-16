//! Compile-only external consumer of a real session with host-owned transport.
#[path = "support/local_transport.rs"]
mod local_transport;

use local_transport::LocalTransport;
use minimium_client_runtime::{
    ClientAuthentication, ClientSession, DisconnectReason, QueueError, SessionEvent, SessionState,
};
use minimium_protocol::{ClientMessage, ServerMessage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = LocalTransport::default();
    let peer = transport.peer();
    let authentication = ClientAuthentication::new("local", b"bounded credential".to_vec())?;
    let mut session = ClientSession::new(Box::new(transport), authentication)?;

    // Session policy remains authoritative over the injected port.
    match session.queue(&ClientMessage::Ping) {
        Err(QueueError::AwaitingWelcome) => {}
        outcome => return Err(format!("unexpected handshake queue outcome: {outcome:?}").into()),
    }
    let mut events = Vec::new();
    if peer.receive_client().is_some() {
        return Err("queue acceptance must precede peer delivery".into());
    }
    let _state = session.poll(&mut events);
    // The peer receives accepted frames only after the port made flush progress.
    match (peer.receive_client(), peer.receive_client()) {
        (Some(ClientMessage::Presentation(_)), Some(ClientMessage::Authenticate { .. })) => {}
        messages => return Err(format!("unexpected initial messages: {messages:?}").into()),
    }
    // The host can submit further messages after the session owns the adapter.
    peer.try_send_server(ServerMessage::Bye)?;
    let state = session.poll(&mut events);
    if state != SessionState::Disconnected
        || !events.iter().any(|event| {
            matches!(
                event,
                SessionEvent::Disconnected(DisconnectReason::UnexpectedMessage)
            )
        })
    {
        return Err("session did not reject Bye during its handshake".into());
    }
    session.close();
    Ok(())
}
