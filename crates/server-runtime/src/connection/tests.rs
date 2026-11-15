use super::*;
use minimium_transport::{HostTransport, TransportConfig, TransportEvent, TransportPort};
use std::net::TcpStream;

fn client_without_io() -> (Client, HostTransport, TcpStream) {
    let mut transport = HostTransport::bind(TransportConfig {
        bind_address: "127.0.0.1".to_owned(),
        game_port: 0,
        admin_token: "a".repeat(64),
        max_connections: 1,
    })
    .unwrap();
    let peer = TcpStream::connect(transport.game_address()).unwrap();
    let now = Instant::now();
    let events = transport.poll(now).unwrap();
    let connection_id = events
        .into_iter()
        .find_map(|event| match event {
            TransportEvent::Game(minimium_transport::GameTransportEvent::Connected {
                connection_id,
                ..
            }) => Some(connection_id),
            _ => None,
        })
        .unwrap();
    let client = Client::new(connection_id, transport.game_output(), now, None);
    (client, transport, peer)
}

#[test]
fn byte_budget_rejects_before_frame_count_limit() {
    let (mut client, _transport, _peer) = client_without_io();
    let mut metrics = RuntimeMetrics::default();
    let mut accepted = 0;
    while queue_message_with_reserve(
        &mut client,
        &mut metrics,
        ServerMessage::Bye,
        minimium_protocol::MAX_SERVER_FRAME_BYTES,
    ) {
        accepted += 1;
    }
    assert!(accepted > 0);
    assert!(accepted < 256);
    assert!(client.pending_output_bytes() <= 64 * 1024);
}

#[test]
fn frame_count_limit_rejects_before_byte_budget() {
    let (mut client, _transport, _peer) = client_without_io();
    let mut metrics = RuntimeMetrics::default();

    for _ in 0..256 {
        assert!(queue_message_with_reserve(
            &mut client,
            &mut metrics,
            ServerMessage::Bye,
            0,
        ));
    }

    assert!(client.pending_output_bytes() < 64 * 1024);
    assert!(!queue_message_with_reserve(
        &mut client,
        &mut metrics,
        ServerMessage::Bye,
        0,
    ));
    assert_eq!(
        client
            .transport
            .stats(client.connection_id())
            .unwrap()
            .frames_out,
        256
    );
}
