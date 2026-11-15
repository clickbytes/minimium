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

const INPUT_QUEUE_CAPACITY: usize = 64;
const ENVIRONMENT_STATUS_REFRESH: Duration = Duration::from_secs(5);

pub(crate) fn run(mut options: ClientOptions) -> Result<(), Box<dyn Error>> {
    let crate::bootstrap::ClientBootstrap {
        mut diagnostics,
        logger,
        localizer,
        paths,
        worlds,
        connection,
    } = crate::bootstrap::ClientBootstrap::new(&options)?;
    // The resolver retains its catalog; this host has no local scene view.
    drop(worlds);
    let mut text_properties = properties::TextProperties::new(&options);
    if options.startup_settings.sky_quality() != minimium_renderer::quality::SkyQuality::Standard {
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
    let data_dir = paths.data_dir.clone();
    let log_dir = paths.log_dir.clone();
    let parent_run_id = diagnostics.run_id().to_owned();

    println!(
        "{}",
        Text::TerminalIntro.localized(&localizer, options.locale)
    );
    let (sender, receiver) = mpsc::sync_channel(INPUT_QUEUE_CAPACITY);
    super::spawn_console_input_reader(sender, logger.clone());
    let mut connection = connection.for_host(crate::bootstrap::HostCapabilities::CpuOnly);
    let mut action_executor = crate::application::ClientActionExecutor::default();
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
                    SessionEvent::Message(
                        message @ ServerMessage::TransitionPrepare {
                            transition_id,
                            target_context,
                            ..
                        },
                    ) => {
                        let ticket = minimium_client_runtime::TransferTicket::new(
                            transition_id,
                            target_context,
                        );
                        let _ = connection.report_transfer_host_ready(ticket);
                        events::print_session_event(
                            SessionEvent::Message(message),
                            false,
                            &localizer,
                            options.locale,
                        );
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
                        action_executor: &mut action_executor,
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
                    #[cfg(feature = "desktop")]
                    CommandResult::Suppressed(_) => {}
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
        StartError::Busy {
            active_attempt: Some(active_attempt),
        } => {
            println!(
                "{}",
                Text::BusyAttempt {
                    attempt_id: active_attempt.get()
                }
                .localized(localizer, locale)
            )
        }
        StartError::Busy {
            active_attempt: None,
        } => println!("{}", Text::DisconnectOrWait.localized(localizer, locale)),
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
