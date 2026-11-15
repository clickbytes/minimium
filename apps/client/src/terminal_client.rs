use crate::{
    client_commands::{execute_text, help_lines, CommandContext, CommandSource, TextCommandResult},
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
        let result = start_remote(&mut connection, address, &localizer, options.locale);
        format_startup_output(result, &localizer, options.locale);
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
                let result = execute_text(
                    &line,
                    CommandSource::Terminal,
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
                    TextCommandResult::Empty => {}
                    TextCommandResult::Rejected(error) => println!(
                        "{}",
                        crate::client_commands::command_error(&error, &localizer, options.locale)
                    ),
                    TextCommandResult::Queued { .. } => {}
                    TextCommandResult::QueueRejected { error, .. } => {
                        println!(
                            "{}",
                            Text::CouldNotQueueCommand(error.into())
                                .localized(&localizer, options.locale)
                        )
                    }
                    #[cfg(feature = "desktop")]
                    TextCommandResult::Suppressed(_) => {}
                    TextCommandResult::Action(action) => {
                        let (should_exit, result) = actions::handle(
                            action,
                            &mut options,
                            &mut connection,
                            &mut last_snapshot,
                            &localizer,
                            &data_dir,
                            &log_dir,
                            &parent_run_id,
                            &mut text_properties,
                        );
                        for output in result.outputs {
                            match output {
                                crate::client_commands::CommandOutput::Line(line) => {
                                    println!("{line}")
                                }
                                #[cfg(feature = "desktop")]
                                crate::client_commands::CommandOutput::UiMessage(message)
                                | crate::client_commands::CommandOutput::UiError(message) => {
                                    println!("{}", localizer.format(options.locale, &message))
                                }
                                crate::client_commands::CommandOutput::ClientText {
                                    message,
                                    ..
                                } => {
                                    println!("{}", message.localized(&localizer, options.locale));
                                }
                                crate::client_commands::CommandOutput::Clear => {
                                    print!("\x1B[2J\x1B[H")
                                }
                                crate::client_commands::CommandOutput::Presentation(result) => {
                                    match result {
                                        #[cfg(feature = "desktop")]
                                        crate::client_commands::PresentationResult::Output(
                                            lines,
                                        ) => {
                                            for line in lines {
                                                println!("{line}");
                                            }
                                        }
                                        crate::client_commands::PresentationResult::Unavailable(
                                            capability,
                                        ) => {
                                            let notice = match capability {
                                            crate::client_commands::PresentationCapability::DeveloperRenderer
                                            | crate::client_commands::PresentationCapability::WeatherDiagnostics => minimium_localization::UiNotice::WeatherRequiresDesktop,
                                            crate::client_commands::PresentationCapability::SkyDiagnostics => minimium_localization::UiNotice::SkyInfoDesktopOnly,
                                            crate::client_commands::PresentationCapability::MeteorDiagnostics => minimium_localization::UiNotice::MeteorsRequireDesktop,
                                        };
                                            println!(
                                                "{}",
                                                localizer.format(
                                                    options.locale,
                                                    &minimium_localization::UiMessage::Notice(
                                                        notice
                                                    )
                                                )
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        if should_exit {
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

pub(super) fn format_startup_output(
    result: crate::client_commands::CommandActionResult,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) {
    for output in result.outputs {
        if let crate::client_commands::CommandOutput::ClientText { message, .. } = output {
            println!("{}", message.localized(localizer, locale));
        }
    }
}
pub(super) fn start_remote(
    connection: &mut ConnectionController,
    address: &str,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> crate::client_commands::CommandActionResult {
    match connection.start_remote(address.to_owned()) {
        Ok(()) => crate::client_commands::CommandActionResult {
            outputs: vec![crate::client_commands::CommandOutput::ClientText {
                message: Text::ConnectingTo(address.to_owned()),
                error: false,
            }],
        },
        Err(error) => start_error_output(error, localizer, locale),
    }
}

pub(super) fn start_error_output(
    error: StartError,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> crate::client_commands::CommandActionResult {
    let message = match error {
        StartError::Busy {
            active_attempt: Some(active_attempt),
        } => Text::BusyAttempt {
            attempt_id: active_attempt.get(),
        },
        StartError::Busy {
            active_attempt: None,
        } => Text::DisconnectOrWait,
        StartError::AttemptIdsExhausted => Text::AttemptIdsExhausted,
        StartError::WorkerSpawn(error) => Text::WorkerStartFailed(error.to_string()),
    };
    let _ = (localizer, locale);
    crate::client_commands::CommandActionResult {
        outputs: vec![crate::client_commands::CommandOutput::ClientText {
            message,
            error: true,
        }],
    }
}
