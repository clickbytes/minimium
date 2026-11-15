use crate::{
    client_commands::{execute, help_lines, parse_command_typed, CommandContext, CommandResult},
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

mod actions;
mod events;
mod properties;
mod world_setup;

const INPUT_QUEUE_CAPACITY: usize = 64;
const ENVIRONMENT_STATUS_REFRESH: Duration = Duration::from_secs(5);

pub(crate) fn run(mut options: ClientOptions) -> Result<(), Box<dyn Error>> {
    let initial_log_dir = options.resolved_log_dir()?;
    let mut diagnostics = minimium_telemetry::logging::DiagnosticsRuntime::start(&initial_log_dir)?;
    let logger = diagnostics.logger(minimium_diagnostics::DomainId::ClientRuntime);
    let localizer =
        minimium_localization::Localizer::new(Some(logger.clone())).map_err(io::Error::other)?;
    let asset_io_logger = diagnostics.logger(minimium_diagnostics::DomainId::AssetIo);
    let procgen_logger = diagnostics.logger(minimium_diagnostics::DomainId::Procgen);
    let mut text_properties = properties::TextProperties::new(&options);
    if options.sky_quality != minimium_renderer::quality::SkyQuality::Standard {
        println!(
            "{}",
            localizer.format(
                options.locale,
                &minimium_localization::UiMessage::Notice(
                    minimium_localization::UiNotice::SkyQualityDesktopOnly
                )
            )
        );
    }
    let world_setup = world_setup::resolve(
        &options,
        &asset_io_logger,
        &procgen_logger,
        &localizer,
        options.locale,
    )?;
    let map = world_setup.map;
    let fingerprint = map.fingerprint();
    let paths = options.paths(fingerprint)?;
    let profile_id = minimium_platform::identity_store::load_or_create(&paths.profile_dir)?;
    let data_dir = paths.data_dir.clone();
    let cache_dir = paths.cache_dir.clone();
    let log_dir = paths.log_dir.clone();
    logger.emit(logger.event(
        minimium_diagnostics::Level::Info,
        minimium_diagnostics::EventCode::new("client.starting")?,
    ));
    let parent_run_id = diagnostics.run_id().to_owned();
    let earth_package_dir =
        minimium_platform::app_paths::AppPaths::resolve_path(&options.earth_package_dir)?;

    println!(
        "{}",
        Text::TerminalIntro.localized(&localizer, options.locale)
    );
    let (sender, receiver) = mpsc::sync_channel(INPUT_QUEUE_CAPACITY);
    super::spawn_console_input_reader(sender, logger.clone());
    let mut connection = ConnectionController::new();
    connection.register_replica_schema(
        minimium_game_sample::PORTAL_COMPONENT,
        minimium_game_sample::PORTAL_SCHEMA,
    )?;
    connection.set_authentication(minimium_client_runtime::ClientAuthentication::new(
        "dev.profile",
        profile_id.as_bytes().to_vec(),
    )?);
    connection.set_diagnostic_logger(logger.clone());
    connection.set_content_cache_dir(cache_dir);
    connection.set_content_cache_limit(options.cache_limit_bytes);
    connection.set_world_content_resolver(std::sync::Arc::new(
        crate::world_content::SampleWorldContentResolver::new(
            world_setup.world_catalog,
            world_setup.world_manifests,
            Some(earth_package_dir),
        ),
    ));
    let mut events = Vec::with_capacity(16);
    let mut prompt_needed = true;
    let mut last_snapshot: Option<ServerMessage> = None;
    let mut last_environment_status = None;
    let mut last_environment_status_at = None;

    if let Some(address) = options.connect.as_ref() {
        start_remote(&mut connection, address, &localizer, options.locale);
    }

    loop {
        if let Some(result) = connection.poll_admin_result() {
            match result {
                Ok(response) => println!("{}", response.trim()),
                Err(error) => {
                    connection.record_diagnostic_failure("client.admin_response_failed", &error);
                    println!("{error}");
                }
            }
        }
        match connection.poll_connector() {
            ConnectionEvent::InProgress {
                waiting_for_cleanup: true,
            } => println!(
                "{}",
                Text::WaitingForCancelledCleanup.localized(&localizer, options.locale)
            ),
            ConnectionEvent::TimedOut => println!(
                "{}",
                Text::AttemptTimedOut.localized(&localizer, options.locale)
            ),
            ConnectionEvent::SessionStarted => {
                last_environment_status = None;
                last_environment_status_at = None;
                println!(
                    "{}",
                    Text::AwaitingWelcome.localized(&localizer, options.locale)
                )
            }
            ConnectionEvent::SetupFailed(error) => {
                println!(
                    "{}",
                    Text::ConnectionSetupFailed(error.to_string())
                        .localized(&localizer, options.locale)
                )
            }
            ConnectionEvent::Failed(error) => {
                println!(
                    "{}",
                    Text::ConnectFailed(error.to_string()).localized(&localizer, options.locale)
                )
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
                    other => events::print_session_event(other, false, &localizer, options.locale),
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
                println!(
                    "{}",
                    crate::environment::localized_status_line(frame, &localizer, options.locale,)
                );
                last_environment_status = Some(signature);
                last_environment_status_at = Some(Instant::now());
            }
        } else if last_environment_status.is_some() {
            let unavailable = crate::environment::EnvironmentFrameInput::unavailable();
            let signature = unavailable.status_signature();
            if last_environment_status != Some(signature) {
                println!(
                    "{}",
                    crate::environment::localized_status_line(
                        unavailable,
                        &localizer,
                        options.locale,
                    )
                );
            }
            last_environment_status = Some(signature);
            last_environment_status_at = Some(Instant::now());
        }

        if prompt_needed {
            print!(
                "{}",
                Text::InputPrompt.localized(&localizer, options.locale)
            );
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
                        println!(
                            "{}",
                            crate::client_commands::command_error(
                                &error,
                                &localizer,
                                options.locale,
                            )
                        );
                        continue;
                    }
                };
                let result = execute(
                    command,
                    &mut CommandContext {
                        connection: &mut connection,
                        fire_direction: [1.0, 0.0, 0.0],
                        player_position: last_snapshot.as_ref().and_then(|message| match message {
                            ServerMessage::Snapshot { self_position, .. } => Some(*self_position),
                            _ => None,
                        }),
                    },
                );
                match result {
                    CommandResult::Sent => {}
                    CommandResult::QueueError(error) => {
                        println!(
                            "{}",
                            Text::CouldNotQueueCommand(error.into())
                                .localized(&localizer, options.locale)
                        )
                    }
                    CommandResult::Action(action) => {
                        if actions::handle(
                            action,
                            &mut options,
                            &mut connection,
                            &mut last_snapshot,
                            &localizer,
                            &data_dir,
                            &log_dir,
                            &parent_run_id,
                            &mut text_properties,
                        )? {
                            break;
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let shutdown_deadline = Instant::now() + Duration::from_secs(30);
    let cleanup_result = connection.finish_until(shutdown_deadline - Duration::from_secs(2));
    drop(connection);
    drop(localizer);
    drop(logger);
    let diagnostics_result = diagnostics.shutdown_until(shutdown_deadline);
    cleanup_result?;
    diagnostics_result?;
    Ok(())
}
pub(super) fn start_remote(
    connection: &mut ConnectionController,
    address: &str,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) {
    match connection.start_remote(address.to_owned()) {
        Ok(()) => println!(
            "{}",
            Text::ConnectingTo(address.to_owned()).localized(localizer, locale)
        ),
        Err(error) => print_start_error(error, localizer, locale),
    }
}

pub(super) fn print_start_error(
    error: StartError,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) {
    match error {
        StartError::Busy { active_attempt } => {
            println!(
                "{}",
                Text::BusyAttempt {
                    attempt_id: active_attempt.get()
                }
                .localized(localizer, locale)
            )
        }
        StartError::AttemptIdsExhausted => {
            println!("{}", Text::AttemptIdsExhausted.localized(localizer, locale))
        }
        StartError::WorkerSpawn(error) => {
            println!(
                "{}",
                Text::WorkerStartFailed(error.to_string()).localized(localizer, locale)
            )
        }
    }
}
