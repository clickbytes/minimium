use crate::{
    client_commands::{
        execute, help_lines, parse_command, CommandContext, CommandResult, FrontendAction,
    },
    connection_controller::{ConnectionController, ConnectionEvent},
    connector::StartError,
    messages::ClientText as Text,
    options::ClientOptions,
    session::{SessionEvent, SessionState},
};
use minimium_protocol::ServerMessage;
use std::{
    error::Error,
    io::{self, Write},
    sync::mpsc,
    time::Duration,
};

const INPUT_QUEUE_CAPACITY: usize = 64;

pub(crate) fn run(options: ClientOptions) -> Result<(), Box<dyn Error>> {
    let map_path = options.map_path.clone();
    let world_definition_path = options.world_definition_path.clone();
    let (map, world_fingerprints) = if let Some(path) = map_path.as_deref() {
        let map = minimium_assets::load_map(path).map_err(|error| {
            Text::MapLoadFailed {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
            .to_string()
        })?;
        let instance =
            minimium_core::world::WorldInstanceId::new(1).expect("legacy instance ID is nonzero");
        (
            map.clone(),
            std::collections::HashMap::from([(instance, map.fingerprint())]),
        )
    } else if let Some(path) = world_definition_path.as_deref() {
        let definition = minimium_assets::load_world_definition(path).map_err(|error| {
            Text::MapLoadFailed {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
            .to_string()
        })?;
        let map = minimium_procgen::materialize_dungeon_map(definition.dungeon())
            .map_err(|error| Text::BundledMapInvalid(error).to_string())?;
        let instance = minimium_core::world::WorldInstanceId::new(4)
            .expect("manual world instance ID is nonzero");
        let legacy_map = minimium_assets::parse_map(crate::SAMPLE_MAP)
            .map_err(|error| Text::BundledMapInvalid(error).to_string())?;
        let legacy_instance =
            minimium_core::world::WorldInstanceId::new(3).expect("legacy instance ID is nonzero");
        (
            map.clone(),
            std::collections::HashMap::from([
                (instance, map.fingerprint()),
                (legacy_instance, legacy_map.fingerprint()),
            ]),
        )
    } else {
        let worlds = minimium_procgen::generate_demo_worlds()
            .map_err(|error| Text::BundledMapInvalid(error).to_string())?;
        let mut fingerprints = std::collections::HashMap::new();
        for world in &worlds {
            fingerprints.insert(world.instance, world.map.fingerprint());
        }
        let legacy_map = minimium_assets::parse_map(crate::SAMPLE_MAP)
            .map_err(|error| Text::BundledMapInvalid(error).to_string())?;
        let legacy_instance =
            minimium_core::world::WorldInstanceId::new(3).expect("legacy instance ID is nonzero");
        fingerprints.insert(legacy_instance, legacy_map.fingerprint());
        (worlds[0].map.clone(), fingerprints)
    };
    let fingerprint = map.fingerprint();
    let (profile_dir, data_dir) = options.paths(fingerprint);
    let profile_id = minimium_platform::identity_store::load_or_create(&profile_dir)?;

    println!("{}", Text::TerminalIntro);
    let (sender, receiver) = mpsc::sync_channel(INPUT_QUEUE_CAPACITY);
    super::spawn_console_input_reader(sender);
    let mut connection = ConnectionController::new(profile_id, fingerprint);
    connection.set_world_fingerprints(world_fingerprints);
    let mut events = Vec::with_capacity(16);
    let mut prompt_needed = true;
    let mut last_snapshot: Option<ServerMessage> = None;

    if let Some(address) = options.connect.as_ref() {
        start_remote(&mut connection, address);
    }

    loop {
        match connection.poll_connector() {
            ConnectionEvent::InProgress {
                waiting_for_cleanup: true,
            } => println!("{}", Text::WaitingForCancelledCleanup),
            ConnectionEvent::TimedOut => println!("{}", Text::AttemptTimedOut),
            ConnectionEvent::SessionStarted => println!("{}", Text::AwaitingWelcome),
            ConnectionEvent::SetupFailed(error) => {
                println!("{}", Text::ConnectionSetupFailed(error.to_string()))
            }
            ConnectionEvent::Failed(error) => {
                println!("{}", Text::ConnectFailed(error.to_string()))
            }
            _ => {}
        }
        if connection.session().is_some() {
            events.clear();
            connection.poll_session(&mut events);
            for event in events.drain(..) {
                match event {
                    SessionEvent::Message(message @ ServerMessage::Snapshot { .. }) => {
                        last_snapshot = Some(message);
                    }
                    other => print_session_event(other, false),
                }
            }
            if connection
                .session()
                .is_some_and(|current| current.state() == SessionState::Disconnected)
            {
                connection.take_session();
                last_snapshot = None;
            }
        }

        if prompt_needed {
            print!("{}", Text::InputPrompt);
            io::stdout().flush()?;
            prompt_needed = false;
        }
        match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(line) => {
                prompt_needed = true;
                let command = match parse_command(&line) {
                    Ok(Some(command)) => command,
                    Ok(None) => continue,
                    Err(error) => {
                        println!("{}", Text::CommandParseError(error));
                        continue;
                    }
                };
                let result = execute(
                    command,
                    &mut CommandContext {
                        connection: &mut connection,
                        fire_direction: [1.0, 0.0, 0.0],
                    },
                );
                match result {
                    CommandResult::Sent => {}
                    CommandResult::QueueError(error) => {
                        println!("{}", Text::CouldNotQueueCommand(error.into()))
                    }
                    CommandResult::Action(action) => match action {
                        FrontendAction::Help => {
                            for line in help_lines() {
                                println!("{line}");
                            }
                            println!("{}", Text::FpsUnavailable);
                        }
                        FrontendAction::HostLocal(requested_lanes) => {
                            if connection.busy() {
                                println!("{}", Text::DisconnectOrWait);
                                continue;
                            }
                            let lanes = requested_lanes.unwrap_or(options.threads);
                            match connection.start_local(
                                data_dir.clone(),
                                map_path.clone(),
                                world_definition_path.clone(),
                                lanes,
                            ) {
                                Ok(()) => {
                                    println!("{}", Text::StartingLocalServer);
                                }
                                Err(error) => print_start_error(error),
                            }
                        }
                        FrontendAction::Connect(address) => {
                            if connection.busy() {
                                println!("{}", Text::DisconnectOrWait);
                            } else {
                                start_remote(&mut connection, &address);
                            }
                        }
                        FrontendAction::Disconnect => {
                            connection.cancel();
                            if let Some(mut current) = connection.take_session() {
                                current.close();
                            }
                            last_snapshot = None;
                            println!("{}", Text::Disconnected);
                        }
                        FrontendAction::State => {
                            if connection.session().is_none() {
                                println!("{}", Text::NotConnected);
                            } else if let Some(snapshot) = &last_snapshot {
                                print_session_event(SessionEvent::Message(snapshot.clone()), true);
                            } else {
                                println!("{}", Text::NoServerResponse);
                            }
                        }
                        FrontendAction::FpsShow => {
                            println!("{}", Text::FpsUnavailable);
                        }
                        FrontendAction::NetStats => match connection.session() {
                            Some(current) => {
                                let metrics = current.metrics();
                                println!(
                                    "{}",
                                    Text::NetworkStats {
                                        bytes_sent: metrics.bytes_sent,
                                        bytes_received: metrics.bytes_received,
                                        ping_milliseconds: metrics
                                            .ping_round_trip
                                            .map(|duration| duration.as_millis()),
                                        nearby_players: None,
                                    }
                                );
                            }
                            None => println!("{}", Text::NotConnected),
                        },
                        FrontendAction::Clear => {
                            print!("\x1B[2J\x1B[H");
                            io::stdout().flush()?;
                        }
                        FrontendAction::Quit => break,
                        FrontendAction::MapTeleport => {
                            println!("map_telepo is available in the desktop Earth overview");
                        }
                    },
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    connection.finish()?;
    Ok(())
}

fn start_remote(connection: &mut ConnectionController, address: &str) {
    match connection.start_remote(address.to_owned()) {
        Ok(()) => println!("{}", Text::ConnectingTo(address.to_owned())),
        Err(error) => print_start_error(error),
    }
}

fn print_start_error(error: StartError) {
    match error {
        StartError::Busy { active_attempt } => {
            println!(
                "{}",
                Text::BusyAttempt {
                    attempt_id: active_attempt.get()
                }
            )
        }
        StartError::AttemptIdsExhausted => {
            println!("{}", Text::AttemptIdsExhausted)
        }
        StartError::WorkerSpawn(error) => {
            println!("{}", Text::WorkerStartFailed(error.to_string()))
        }
    }
}

fn print_session_event(event: SessionEvent, show_snapshot: bool) {
    match event {
        SessionEvent::PingRoundTrip(duration) => {
            println!(
                "{}",
                Text::Ping {
                    milliseconds: duration.as_millis()
                }
            );
        }
        SessionEvent::Disconnected(reason) => println!("{}", Text::from(reason)),
        SessionEvent::Message(message) => match message {
            ServerMessage::Welcome { id, health, .. } => {
                println!("{}", Text::Connected { id, health });
            }
            ServerMessage::Error(code) => {
                println!("{}", Text::ServerError { code });
            }
            ServerMessage::Bye => {
                println!("{}", Text::ServerClosed);
            }
            ServerMessage::Pong => println!("{}", Text::Pong),
            ServerMessage::Accepted => println!("{}", Text::Accepted),
            ServerMessage::Chat {
                sender_id,
                sender_name,
                text,
            } => {
                println!(
                    "{}",
                    Text::Chat {
                        sender_name,
                        sender_id,
                        text,
                    }
                );
            }
            ServerMessage::HealthUpdate(health) => {
                println!("{}", Text::Health { health });
            }
            ServerMessage::ShotResult {
                target_id,
                remaining_health,
                killed,
            } => match (target_id, remaining_health, killed) {
                (Some(id), Some(health), true) => {
                    println!(
                        "{}",
                        Text::TargetRespawned {
                            target_id: id,
                            health
                        }
                    );
                }
                (Some(id), Some(health), false) => {
                    println!(
                        "{}",
                        Text::Hit {
                            target_id: id,
                            health
                        }
                    );
                }
                _ => println!("{}", Text::ShotMissed),
            },
            ServerMessage::Snapshot {
                server_time_ms,
                full_snapshot,
                self_position,
                self_velocity,
                self_grounded,
                players,
                removed_players,
                ..
            } if show_snapshot => {
                println!(
                    "{}",
                    Text::TerminalSnapshot {
                        server_time_ms,
                        full_snapshot,
                        self_position,
                        self_velocity,
                        grounded: self_grounded,
                        player_updates: players.len(),
                        player_removals: removed_players.len(),
                    }
                );
            }
            ServerMessage::Snapshot { .. } => {}
            ServerMessage::TransitionPrepare { .. } => println!("world transfer prepared"),
            ServerMessage::TransitionCommitted { .. } => println!("world transfer committed"),
            ServerMessage::TransitionCancelled { .. } => println!("world transfer cancelled"),
            ServerMessage::MapTeleportResult { .. } => {}
        },
    }
}
