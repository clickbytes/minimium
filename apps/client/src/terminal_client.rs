use crate::{
    bootstrap::LocalServer,
    client_commands::{
        execute, help_lines, parse_command, CommandContext, CommandResult, FrontendAction,
    },
    connector::{AttemptId, Connector, PollEvent, StartError},
    messages::ClientText as Text,
    options::ClientOptions,
    session::{ClientSession, SessionEvent, SessionState},
};
use minimium_core::{identity::PlayerProfileId, world::WorldMap};
use minimium_protocol::ServerMessage;
use std::{
    error::Error,
    io::{self, Write},
    sync::mpsc,
    time::Duration,
};

const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);
const INPUT_QUEUE_CAPACITY: usize = 64;

pub(crate) fn run(options: ClientOptions) -> Result<(), Box<dyn Error>> {
    let map_path = options.map_path.clone();
    let map = if let Some(path) = map_path.as_deref() {
        WorldMap::load_file(path).map_err(|error| {
            Text::MapLoadFailed {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
            .to_string()
        })?
    } else {
        WorldMap::parse(crate::SAMPLE_MAP)
            .map_err(|error| Text::BundledMapInvalid(error.to_string()).to_string())?
    };
    let fingerprint = map.fingerprint();
    let (profile_dir, data_dir) = options.paths(fingerprint);
    let profile_id = minimium_platform::identity_store::load_or_create(&profile_dir)?;

    println!("{}", Text::TerminalIntro);
    let (sender, receiver) = mpsc::sync_channel(INPUT_QUEUE_CAPACITY);
    super::spawn_console_input_reader(sender);
    let mut connector = Connector::default();
    let mut active_attempt: Option<AttemptId> = None;
    let mut session: Option<ClientSession> = None;
    let mut local_server: Option<LocalServer> = None;
    let mut events = Vec::with_capacity(16);
    let mut prompt_needed = true;
    let mut last_snapshot: Option<ServerMessage> = None;

    if let Some(address) = options.connect.as_ref() {
        start_remote(&mut connector, &mut active_attempt, address);
    }

    loop {
        poll_connector(
            &mut connector,
            &mut active_attempt,
            &mut session,
            &mut local_server,
            &profile_id,
            fingerprint,
        );
        if let Some(current) = session.as_mut() {
            events.clear();
            current.poll(&mut events);
            for event in events.drain(..) {
                match event {
                    SessionEvent::Message(message @ ServerMessage::Snapshot { .. }) => {
                        last_snapshot = Some(message);
                    }
                    other => print_session_event(other, false),
                }
            }
            if current.state() == SessionState::Disconnected {
                session = None;
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
                        session: &mut session,
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
                            if session.is_some() || active_attempt.is_some() {
                                println!("{}", Text::DisconnectOrWait);
                                continue;
                            }
                            let lanes = requested_lanes.unwrap_or(options.threads);
                            match connector.start_local(
                                data_dir.clone(),
                                map_path.clone(),
                                lanes,
                                ATTEMPT_TIMEOUT,
                            ) {
                                Ok(id) => {
                                    active_attempt = Some(id);
                                    println!("{}", Text::StartingLocalServer);
                                }
                                Err(error) => print_start_error(error),
                            }
                        }
                        FrontendAction::Connect(address) => {
                            if session.is_some() || active_attempt.is_some() {
                                println!("{}", Text::DisconnectOrWait);
                            } else {
                                start_remote(&mut connector, &mut active_attempt, &address);
                            }
                        }
                        FrontendAction::Disconnect => {
                            if let Some(id) = active_attempt.take() {
                                connector.cancel(id);
                            }
                            if let Some(mut current) = session.take() {
                                current.close();
                            }
                            last_snapshot = None;
                            println!("{}", Text::Disconnected);
                        }
                        FrontendAction::State => {
                            if session.is_none() {
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
                        FrontendAction::NetStats => match session.as_ref() {
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
                    },
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    if let Some(id) = active_attempt.take() {
        connector.cancel(id);
    }
    connector.finish_on_exit();
    drop(session.take());
    if let Some(mut server) = local_server.take() {
        server.shutdown()?;
    }
    Ok(())
}

fn start_remote(connector: &mut Connector, active: &mut Option<AttemptId>, address: &str) {
    match connector.start(address.to_owned(), ATTEMPT_TIMEOUT) {
        Ok(id) => {
            *active = Some(id);
            println!("{}", Text::ConnectingTo(address.to_owned()));
        }
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

fn poll_connector(
    connector: &mut Connector,
    active: &mut Option<AttemptId>,
    session: &mut Option<ClientSession>,
    local_server: &mut Option<LocalServer>,
    profile_id: &PlayerProfileId,
    fingerprint: u64,
) {
    match connector.poll() {
        PollEvent::Idle => {
            *active = None;
        }
        PollEvent::InProgress {
            attempt_id,
            discarding_result,
        } => {
            if discarding_result && *active != Some(attempt_id) {
                println!("{}", Text::WaitingForCancelledCleanup);
            }
            *active = Some(attempt_id);
        }
        PollEvent::TimedOut { attempt_id } if *active == Some(attempt_id) => {
            println!("{}", Text::AttemptTimedOut);
        }
        PollEvent::Cancelled { attempt_id } if *active == Some(attempt_id) => {
            *active = None;
        }
        PollEvent::Connected {
            attempt_id,
            stream,
            local_server: server,
        } if *active == Some(attempt_id) => {
            *active = None;
            if let Some(server) = server {
                super::retain_local_server(local_server, server);
            }
            match ClientSession::new(stream, *profile_id, fingerprint) {
                Ok(connected) => {
                    *session = Some(connected);
                    println!("{}", Text::AwaitingWelcome);
                }
                Err(error) => println!("{}", Text::ConnectionSetupFailed(error.to_string())),
            }
        }
        PollEvent::Failed { attempt_id, error } if *active == Some(attempt_id) => {
            *active = None;
            println!("{}", Text::ConnectFailed(error.to_string()));
        }
        _ => {}
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
        },
    }
}
