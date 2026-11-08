use super::*;
use std::net::{TcpListener, TcpStream};

fn client_without_io() -> Client {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (server, _) = listener.accept().unwrap();
    drop(peer);
    Client::new(server, Instant::now(), None)
}

#[test]
fn byte_budget_rejects_before_frame_count_limit() {
    let mut client = client_without_io();
    let mut metrics = RuntimeMetrics::default();
    let frame = [0; 1024];

    for _ in 0..MAX_QUEUED_OUTPUT_BYTES / frame.len() {
        assert!(queue_encoded_frame(&mut client, &frame, &mut metrics));
    }

    assert_eq!(client.output_frames.len(), 64);
    assert_eq!(client.pending_output_bytes(), MAX_QUEUED_OUTPUT_BYTES);
    assert!(!queue_encoded_frame(&mut client, &[0], &mut metrics));
    assert!(!queue_message(
        &mut client,
        &mut metrics,
        ServerMessage::Bye
    ));
    assert_eq!(client.output_frames.len(), 64);
}

#[test]
fn frame_count_limit_rejects_before_byte_budget() {
    let mut client = client_without_io();
    let mut metrics = RuntimeMetrics::default();

    for _ in 0..MAX_QUEUED_OUTPUT_FRAMES {
        assert!(queue_encoded_frame(&mut client, &[0], &mut metrics));
    }

    assert_eq!(client.pending_output_bytes(), MAX_QUEUED_OUTPUT_FRAMES);
    assert!(!queue_encoded_frame(&mut client, &[0], &mut metrics));
    assert!(!queue_message(
        &mut client,
        &mut metrics,
        ServerMessage::Bye
    ));
    assert_eq!(client.output_frames.len(), MAX_QUEUED_OUTPUT_FRAMES);
}
