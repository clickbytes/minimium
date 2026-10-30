use super::*;
use minimium_core::{
    ids::PlayerId,
    world::{
        GenerationRevision, GenerationSpec, GeneratorDigest, MembershipEpoch, TransitionId,
        WorldContentRevision, WorldDefinitionId, WorldInstanceId, WorldManifest, WorldSeed,
        WorldTopology,
    },
};

fn manifest(topology: WorldTopology) -> WorldManifest {
    let generation = GenerationSpec::new(
        WorldSeed::new(0x1234),
        GenerationRevision::new(2).unwrap(),
        10,
    )
    .unwrap();
    WorldManifest::new(
        WorldDefinitionId::new(5).unwrap(),
        topology,
        generation,
        WorldContentRevision::new(4).unwrap(),
        GeneratorDigest::new([0x5a; 32]),
    )
}

fn context(instance: u128, epoch: u64) -> WorldContext {
    WorldContext::new(
        WorldInstanceId::new(instance).unwrap(),
        WorldContentRevision::new(4).unwrap(),
        MembershipEpoch::new(epoch).unwrap(),
    )
}

#[test]
fn welcome_and_transition_frames_round_trip_full_world_context() {
    let welcome = ServerMessage::Welcome {
        id: PlayerId::new(42),
        map_fingerprint: 0x1234,
        health: 77,
        position: [1.25, 8.5, -3.0],
        position_mm: minimium_core::world::Position3Mm([1250, 8500, -3000]),
        context: context(1, 9),
        manifest: manifest(WorldTopology::Planet),
        transition_id: None,
    };
    let frame = encode_server(&welcome).unwrap();
    let (decoded, consumed) = decode_server(&frame).unwrap().unwrap();
    assert_eq!(decoded, welcome);
    assert_eq!(consumed, frame.len());

    let transition = ServerMessage::TransitionPrepare {
        transition_id: TransitionId::new(17).unwrap(),
        source_context: context(1, 9),
        target_context: context(2, 10),
        map_fingerprint: 0x9876,
        destination: [3.0, 0.0, 5.0],
        destination_mm: minimium_core::world::Position3Mm([3000, 0, 5000]),
        manifest: manifest(WorldTopology::Dungeon),
    };
    let frame = encode_server(&transition).unwrap();
    let (decoded, consumed) = decode_server(&frame).unwrap().unwrap();
    assert_eq!(decoded, transition);
    assert_eq!(consumed, frame.len());
}

#[test]
fn client_world_inputs_and_transition_acknowledgements_keep_their_epoch() {
    let movement = ClientMessage::WorldInput {
        context: context(2, 10),
        input: ClientWorldInput::Move {
            direction: [0.25, -1.0],
            jump: true,
        },
    };
    let frame = encode_client(&movement).unwrap();
    let (decoded, consumed) = decode_client(&frame).unwrap().unwrap();
    assert_eq!(decoded, movement);
    assert_eq!(consumed, frame.len());

    let acknowledgement = ClientMessage::TransitionReady {
        transition_id: TransitionId::new(17).unwrap(),
        target_context: context(2, 10),
    };
    let frame = encode_client(&acknowledgement).unwrap();
    let (decoded, consumed) = decode_client(&frame).unwrap().unwrap();
    assert_eq!(decoded, acknowledgement);
    assert_eq!(consumed, frame.len());
}
