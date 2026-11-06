use crate::{
    client_commands::{
        execute, help_lines, parse_command_typed, CommandContext, CommandResult, FrontendAction,
    },
    messages::ClientText as Text,
    options::ClientOptions,
    session::{SessionEvent, SessionState},
};
use minimium_client_runtime::{connector::StartError, ConnectionController, ConnectionEvent};
use minimium_protocol::ServerMessage;
use std::{
    error::Error,
    io::{self, Write},
    sync::mpsc,
    time::{Duration, Instant},
};

mod properties;

const INPUT_QUEUE_CAPACITY: usize = 64;
const ENVIRONMENT_STATUS_REFRESH: Duration = Duration::from_secs(5);

pub(crate) fn run(mut options: ClientOptions) -> Result<(), Box<dyn Error>> {
    let mut text_properties = properties::TextProperties::new();
    if options.sky_quality != minimium_renderer::quality::SkyQuality::Standard {
        println!("--sky-quality is available only in the desktop renderer");
    }
    let map_path = options.map_path.clone();
    let world_definition_path = options.world_definition_path.clone();
    let (map, world_fingerprints) = if let Some(path) = map_path.as_deref() {
        let map = minimium_asset_io::load_map(path).map_err(|error| {
            Text::MapLoadFailed {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
            .to_string()
        })?;
        let instance =
            minimium_world::world::WorldInstanceId::new(1).expect("legacy instance ID is nonzero");
        (
            map.clone(),
            std::collections::HashMap::from([(instance, map.fingerprint())]),
        )
    } else if let Some(path) = world_definition_path.as_deref() {
        let definition = minimium_asset_io::load_world_definition(path).map_err(|error| {
            Text::MapLoadFailed {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
            .to_string()
        })?;
        minimium_procgen::validate_world_manifest(definition.manifest())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let map = minimium_procgen::materialize_dungeon_map(definition.dungeon())
            .map_err(|error| Text::BundledMapInvalid(error).to_string())?;
        let instance = minimium_world::world::WorldInstanceId::new(4)
            .expect("manual world instance ID is nonzero");
        let legacy_map = minimium_assets::parse_map(crate::SAMPLE_MAP)
            .map_err(|error| Text::BundledMapInvalid(error).to_string())?;
        let legacy_instance =
            minimium_world::world::WorldInstanceId::new(3).expect("legacy instance ID is nonzero");
        (
            map.clone(),
            std::collections::HashMap::from([
                (instance, map.fingerprint()),
                (legacy_instance, legacy_map.fingerprint()),
            ]),
        )
    } else {
        let worlds = minimium_game_sample::generate_demo_worlds()
            .map_err(|error| Text::BundledMapInvalid(error).to_string())?;
        let mut fingerprints = std::collections::HashMap::new();
        for world in &worlds {
            fingerprints.insert(world.instance, world.map.fingerprint());
        }
        let legacy_map = minimium_assets::parse_map(crate::SAMPLE_MAP)
            .map_err(|error| Text::BundledMapInvalid(error).to_string())?;
        let legacy_instance =
            minimium_world::world::WorldInstanceId::new(3).expect("legacy instance ID is nonzero");
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
    let mut last_environment_status = None;
    let mut last_environment_status_at = None;

    if let Some(address) = options.connect.as_ref() {
        start_remote(&mut connection, address);
    }

    loop {
        if let Some(result) = connection.poll_time_scale_result() {
            match result {
                Ok(response) => println!("{}", response.trim()),
                Err(error) => println!("time scale: {error}"),
            }
        }
        match connection.poll_connector() {
            ConnectionEvent::InProgress {
                waiting_for_cleanup: true,
            } => println!("{}", Text::WaitingForCancelledCleanup),
            ConnectionEvent::TimedOut => println!("{}", Text::AttemptTimedOut),
            ConnectionEvent::SessionStarted => {
                last_environment_status = None;
                last_environment_status_at = None;
                println!("{}", Text::AwaitingWelcome)
            }
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
        if let Some(frame) = connection
            .session()
            .filter(|session| session.state() == SessionState::Connected)
            .map(|session| session.environment_frame())
        {
            let signature = frame.status_signature();
            let changed = last_environment_status != Some(signature);
            let date_refresh_due = frame.render_world_utc_ms.is_some()
                && last_environment_status_at
                    .is_none_or(|last: Instant| last.elapsed() >= ENVIRONMENT_STATUS_REFRESH);
            if changed || date_refresh_due {
                println!("{}", frame.status_line());
                last_environment_status = Some(signature);
                last_environment_status_at = Some(Instant::now());
            }
        } else if last_environment_status.is_some() {
            let unavailable = crate::environment::EnvironmentFrameInput::unavailable();
            let signature = unavailable.status_signature();
            if last_environment_status != Some(signature) {
                println!("{}", unavailable.status_line());
            }
            last_environment_status = Some(signature);
            last_environment_status_at = Some(Instant::now());
        }

        if prompt_needed {
            print!("{}", Text::InputPrompt);
            io::stdout().flush()?;
            prompt_needed = false;
        }
        match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(line) => {
                prompt_needed = true;
                let command = match parse_command_typed(&line) {
                    Ok(Some(command)) => command,
                    Ok(None) => continue,
                    Err(error) => {
                        println!("{error}");
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
                        FrontendAction::Print(text) => println!("{text}"),
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
                            let config = match options
                                .local_server_launch_config(data_dir.clone(), requested_lanes)
                            {
                                Ok(config) => config,
                                Err(error) => {
                                    println!("{error}");
                                    continue;
                                }
                            };
                            match connection.start_local(config) {
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
                        FrontendAction::SkyInfo => {
                            println!("sky info unavailable: desktop renderer required");
                        }
                        FrontendAction::SkyBodies => {
                            println!("sky bodies unavailable: desktop renderer required");
                        }
                        FrontendAction::Property(command) => properties::print_result(
                            text_properties.execute(command, &mut options, &connection),
                        ),
                        FrontendAction::ConfigSave(path) => {
                            match crate::config::save_preferences(
                                &path,
                                &options.config_file_settings,
                                text_properties.session_settings(),
                            ) {
                                Ok(()) => println!("configuration saved to {}", path.display()),
                                Err(error) => eprintln!("{error}"),
                            }
                        }
                        FrontendAction::WeatherStatus
                        | FrontendAction::AtmosphereStatus
                        | FrontendAction::TerrainStatus => {
                            println!("Regional weather diagnostics require the graphical client.");
                        }
                        FrontendAction::SetTime { minute, local } => {
                            let longitude = match &last_snapshot {
                                Some(ServerMessage::Snapshot {
                                    self_position_mm, ..
                                }) if local => {
                                    let [x, _, z] = self_position_mm.0.map(|value| value as f64);
                                    (x.hypot(z) > 0.0).then(|| z.atan2(x))
                                }
                                _ if !local => Some(0.0),
                                _ => None,
                            };
                            let result = longitude
                                .ok_or_else(|| "player longitude is unavailable".to_owned())
                                .and_then(|longitude| {
                                    connection.request_set_time(minute, longitude)
                                });
                            match result {
                                Ok(()) => println!("time change request sent"),
                                Err(error) => println!("time: {error}"),
                            }
                        }
                        FrontendAction::TimeStatus => {
                            let unavailable =
                                crate::environment::EnvironmentFrameInput::unavailable();
                            let status = connection
                                .session()
                                .map(|session| session.environment_frame().time_status_line())
                                .unwrap_or_else(|| unavailable.time_status_line());
                            println!("{status}");
                        }
                        FrontendAction::TimeScale(value) => {
                            match connection.request_time_scale(value) {
                                Ok(()) => println!("time scale request sent"),
                                Err(error) => println!("time scale: {error}"),
                            }
                        }
                        FrontendAction::TimeMode(mode) => {
                            match connection.request_time_mode(&mode) {
                                Ok(()) => println!("time mode request sent"),
                                Err(error) => println!("time mode: {error}"),
                            }
                        }
                        FrontendAction::TimeOfflinePolicy(policy) => {
                            match connection.request_time_offline_policy(&policy) {
                                Ok(()) => println!("time offline policy request sent"),
                                Err(error) => println!("time offline policy: {error}"),
                            }
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
            ServerMessage::AltitudeResult { position_mm, .. } => {
                println!(
                    "set_altitude: {}",
                    if position_mm.is_some() {
                        "destination reached"
                    } else {
                        "rejected: requires fly on, a planet and a valid height"
                    }
                );
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
            ServerMessage::EnvironmentSync { .. } | ServerMessage::TimeProbeReply { .. } => {}
            ServerMessage::MapTeleportResult { .. } | ServerMessage::ContentOffer { .. } => {}
        },
    }
}
