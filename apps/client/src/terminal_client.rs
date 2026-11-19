use crate::{
    client_commands::{help_lines, output as dispatch, CommandSource},
    messages::ClientText as Text,
    options::ClientOptions,
    session::{SessionEvent, SessionState},
};
use minimium_client_runtime::{connector::StartError, ConnectionEvent};
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
    #[cfg(feature = "desktop")]
    let headless_settings = crate::application::HeadlessApplicationSettings {
        locale: options.locale,
        local_server_config: options.local_server_launch_config(
            data_dir.clone(),
            log_dir.clone(),
            parent_run_id.clone(),
            None,
        )?,
        require_earth: options.require_earth,
        demo: options.demo,
        config_file_settings: options.config_file_settings.clone(),
        startup_settings: options.startup_settings.clone(),
    };
    #[cfg(not(feature = "desktop"))]
    let headless_settings = crate::application::HeadlessApplicationSettings;
    let mut application = crate::application::ClientApplication::new_headless(
        connection.for_host(crate::bootstrap::HostCapabilities::CpuOnly),
        worlds,
        headless_settings,
        crate::host_time::sample(),
    )?;
    let mut prompt_needed = true;
    let mut last_snapshot: Option<ServerMessage> = None;
    let mut last_environment_status = None;
    let mut last_environment_status_at = None;
    let mut cpu_readiness = minimium_client_runtime::world_readiness::WorldReadiness::new()?;

    if let Some(address) = options.connect.as_ref() {
        let result = start_remote(&mut application, address);
        format_startup_output(result, &localizer, options.locale);
    }

    loop {
        let input = crate::application::ClientStepInput {
            now: crate::host_time::sample(),
            prediction: None,
        };
        let step = application.step(input, |_| Ok(None), None)?;
        if let Err(error) = application.retain_terminal_readiness_interests(&mut cpu_readiness) {
            application.record_diagnostic_failure("client.collision_readiness_failed", &error);
        }
        {
            if let Err(error) =
                application.ensure_active_collision(&mut cpu_readiness, crate::host_time::sample())
            {
                application.record_diagnostic_failure("client.collision_readiness_failed", &error);
            }
        }
        for outcome in step.movement_results {
            let (_, result) = application.finish_policy(outcome);
            for output in dispatch::display(Some(result), &localizer, options.locale).outputs {
                if let crate::client_commands::CommandOutput::Line(line) = output {
                    println!("{line}");
                }
            }
        }
        if let Some(reply) = step.admin_result {
            let (_, result) = application.finish_admin(reply);
            for output in dispatch::display(Some(result), &localizer, options.locale).outputs {
                if let crate::client_commands::CommandOutput::Line(line) = output {
                    println!("{line}");
                }
            }
        }
        match step.connection {
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
            ConnectionEvent::SetupFailed(error) => println!(
                "{}",
                Text::ConnectionSetupFailed(error.to_string())
                    .localized(&localizer, options.locale)
            ),
            ConnectionEvent::Failed(error) => println!(
                "{}",
                Text::ConnectFailed(error.to_string()).localized(&localizer, options.locale)
            ),
            _ => {}
        }
        for applied in step.session_events {
            match applied.event {
                SessionEvent::Message(message) => {
                    if let Err(error) = &applied.update {
                        if !matches!(
                            error,
                            crate::application::ApplicationMessageError::StaleWorldContext
                        ) {
                            println!("Client state update failed: {error:?}");
                        }
                    }
                    if applied.update.is_ok() && matches!(message, ServerMessage::Snapshot { .. }) {
                        last_snapshot = Some(message.clone());
                    }
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
        if application.session().is_some() {
            match application
                .ensure_pending_transfer_collision(&mut cpu_readiness, crate::host_time::sample())
            {
                Ok(Some((ticket, snapshot))) => {
                    let _ = application.report_transfer_host_ready(
                        ticket,
                        snapshot,
                        crate::host_time::sample(),
                    );
                }
                Ok(None) => {}
                Err(error) => application
                    .record_diagnostic_failure("client.collision_readiness_failed", &error),
            }
        }
        if let Some(mut disconnect) = step.disconnect {
            #[cfg(not(feature = "desktop"))]
            let _ = (disconnect.previous_context, disconnect.previous_instance);
            if let Some(mut session) = disconnect.session.take() {
                session.close(crate::host_time::sample())?;
            }
            last_snapshot = None;
        }
        if let Some(frame) = application
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
                let result = application.execute_text_command(
                    &line,
                    CommandSource::Terminal,
                    [1.0, 0.0, 0.0],
                    last_snapshot.as_ref().and_then(|message| match message {
                        ServerMessage::Snapshot { self_position, .. } => Some(*self_position),
                        _ => None,
                    }),
                    crate::host_time::sample(),
                );
                let (should_exit, response) = dispatch::dispatch(result, |identity, action| {
                    if let Some(result) =
                        application.policy_operation(&identity, &action, CommandSource::Terminal)
                    {
                        return (false, result);
                    }
                    use crate::client_commands::camera;
                    if let Some(request) = camera::Request::from_action(&action) {
                        return (
                            false,
                            camera::execute(
                                identity,
                                request,
                                &mut text_properties.settings,
                                &options.config_file_settings,
                                None,
                            ),
                        );
                    }
                    if let Some((result, changed)) = text_properties
                        .settings
                        .locale_operation(&identity, &action)
                    {
                        if let Some(locale) = changed {
                            options.locale = locale;
                        }
                        return (false, dispatch::native(result));
                    }
                    if let crate::client_commands::FrontendAction::Clock(clock) = &action {
                        let frame = application
                            .session()
                            .map(|s| s.environment_frame())
                            .unwrap_or_else(crate::environment::EnvironmentFrameInput::unavailable);
                        return (
                            false,
                            application.clock_operation(
                                identity,
                                clock,
                                frame,
                                CommandSource::Terminal,
                            ),
                        );
                    }
                    if let Some(result) =
                        application.admin_help(&identity, &action, CommandSource::Terminal)
                    {
                        return (false, result);
                    }
                    let pending = application.connection_view().pending_admin();
                    let (should_exit, result) = actions::handle(
                        action,
                        &mut options,
                        &mut application,
                        &mut last_snapshot,
                        &localizer,
                        &data_dir,
                        &log_dir,
                        &parent_run_id,
                        &mut text_properties,
                    );
                    if let Some(result) =
                        application.admin_submission(pending, &identity, CommandSource::Terminal)
                    {
                        return (should_exit, result);
                    }
                    (
                        should_exit,
                        dispatch::legacy(identity, result, &localizer, options.locale),
                    )
                });
                let result = dispatch::display(response, &localizer, options.locale);
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
                        crate::client_commands::CommandOutput::ClientText { message, .. } => {
                            println!("{}", message.localized(&localizer, options.locale));
                        }
                        crate::client_commands::CommandOutput::Clear => {
                            print!("\x1B[2J\x1B[H")
                        }
                        crate::client_commands::CommandOutput::Presentation(result) => match result
                        {
                            #[cfg(feature = "desktop")]
                            crate::client_commands::PresentationResult::Output(lines) => {
                                for line in lines {
                                    println!("{line}");
                                }
                            }
                            crate::client_commands::PresentationResult::Unavailable(
                                unavailable,
                            ) => {
                                if unavailable.reason
                                                != crate::client_commands::PresentationUnavailableReason::UnsupportedHost
                                            {
                                                println!("{}", unavailable.message());
                                                continue;
                                            }
                                let notice = match unavailable.capability {
                                            crate::client_commands::PresentationCapability::DeveloperRenderer
                                            | crate::client_commands::PresentationCapability::WeatherDiagnostics => minimium_localization::UiNotice::WeatherRequiresDesktop,
                                            #[cfg(feature = "desktop")]
                                            crate::client_commands::PresentationCapability::EarthMapTeleport => minimium_localization::UiNotice::MapTeleportDesktopOnly,
                                            crate::client_commands::PresentationCapability::SkyDiagnostics => minimium_localization::UiNotice::SkyInfoDesktopOnly,
                                            crate::client_commands::PresentationCapability::MeteorDiagnostics => minimium_localization::UiNotice::MeteorsRequireDesktop,
                                        };
                                println!(
                                    "{}",
                                    localizer.format(
                                        options.locale,
                                        &minimium_localization::UiMessage::Notice(notice)
                                    )
                                );
                            }
                        },
                    }
                }
                if should_exit {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let shutdown_deadline = Instant::now() + Duration::from_secs(30);
    let worker_deadline = shutdown_deadline - Duration::from_secs(2);
    let cleanup_result = application.finish_until(worker_deadline, crate::host_time::sample());
    let unfinished = cpu_readiness.shutdown_until(worker_deadline);
    if !unfinished.is_empty() {
        application.record_diagnostic_failure(
            "client.worker_shutdown_timeout",
            &format!(
                "unfinished CPU readiness workers: {}",
                unfinished.join(", ")
            ),
        );
    }
    drop(application);
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
    application: &mut crate::application::ClientApplication,
    address: &str,
) -> crate::client_commands::CommandActionResult {
    match application.start_remote(address.to_owned(), crate::host_time::sample()) {
        Ok(()) => crate::client_commands::CommandActionResult {
            outputs: vec![crate::client_commands::CommandOutput::ClientText {
                message: Text::ConnectingTo(address.to_owned()),
                error: false,
            }],
        },
        Err(error) => start_error_output(error),
    }
}

pub(super) fn start_error_output(error: StartError) -> crate::client_commands::CommandActionResult {
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
        StartError::Time(error) => Text::WorkerStartFailed(error.to_string()),
    };
    crate::client_commands::CommandActionResult {
        outputs: vec![crate::client_commands::CommandOutput::ClientText {
            message,
            error: true,
        }],
    }
}
