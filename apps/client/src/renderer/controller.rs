mod output;
pub(in crate::renderer) mod presentation;
use super::*;
use crate::client_commands::output as dispatch;
pub(super) use output::format_console_output;

pub(super) fn execute_console_input(renderer: &mut DesktopHost) -> bool {
    let Some(input) = renderer
        .application
        .ui
        .console
        .submission(&renderer.localizer, renderer.application.settings.locale)
    else {
        return false;
    };
    let (exit, result) = execute_input(
        renderer,
        Ok(&input),
        client_commands::CommandSource::Console,
    );
    let output = dispatch::display(
        result,
        &renderer.localizer,
        renderer.application.settings.locale,
    );
    format_console_output(renderer, output);
    exit
}

pub(super) fn execute_input(
    renderer: &mut DesktopHost,
    input: Result<&str, minimium_commands::CommandError>,
    source: client_commands::CommandSource,
) -> (bool, Option<dispatch::ClientResult>) {
    let direction = super::camera::camera_direction(
        renderer.application.core_view().character,
        renderer.input.yaw,
        renderer.input.pitch,
    )
    .normalize()
    .to_array();
    let player_position = renderer.application.core_view().character.position;
    let result = renderer.application.execute_text_command(
        input,
        source,
        direction,
        Some(player_position),
        crate::host_time::sample(),
    );
    execute_submission(renderer, result, source)
}

#[cfg(feature = "console-scripting")]
pub(super) fn execute_script_request(
    renderer: &mut DesktopHost,
    request: crate::scripting::HostRequest,
) {
    let submission = renderer.application.prepare_script_request(&request);
    let (_, result) = execute_submission(
        renderer,
        submission,
        client_commands::CommandSource::Console,
    );
    let reply = crate::scripting::project(result.expect("script operation response"));
    renderer
        .application
        .track_script_submission(&request, &reply);
    let _ = request.reply.try_send(reply);
}

fn execute_submission(
    renderer: &mut DesktopHost,
    result: dispatch::Submission,
    source: client_commands::CommandSource,
) -> (bool, Option<dispatch::ClientResult>) {
    dispatch::dispatch(result, |identity, action| {
        if let FrontendAction::Script(control) = action {
            return (
                false,
                renderer.application.script_operation(identity, control),
            );
        }
        if matches!(&action, FrontendAction::MapTeleport) {
            let unavailable = presentation::unavailable_for(
                renderer,
                client_commands::PresentationCapability::EarthMapTeleport,
                client_commands::PresentationRequirement::ActiveEarthWorld,
            );
            let armed = unavailable.is_none() && renderer.arm_map_teleport();
            let result = if armed {
                dispatch::native(minimium_commands::results::OperationResult::new(
                    identity,
                    minimium_commands::results::OperationReceipt::at(
                        minimium_commands::results::Stage::Completed,
                    ),
                    Ok(minimium_commands::results::Payload::Value(
                        minimium_commands::operation::CommandValue::Unit,
                    )),
                ))
            } else {
                dispatch::failure(
                    identity,
                    minimium_commands::results::Stage::Rejected,
                    "map_selection",
                    minimium_commands::CommandError::new(
                        minimium_commands::CommandErrorCode::Unavailable,
                        Some("ui.map.teleport.arm"),
                        unavailable.map_or_else(
                            || "map selection unavailable".to_owned(),
                            |u| u.message(),
                        ),
                        None::<String>,
                    ),
                )
            };
            return (false, result);
        }
        if let FrontendAction::Player(intent) = &action {
            let result = renderer.application.player_operation(
                identity,
                intent,
                source,
                crate::host_time::sample(),
            );
            if let Some((source, old)) = renderer.application.retire_player_receipt() {
                renderer.show_operation_result(Some(source), old);
            }
            return (false, result);
        }
        let local = &renderer.application.settings.local_server_config;
        let config = matches!(action, FrontendAction::HostLocal(_)).then(|| local.clone());
        if let Some((effect, result)) = renderer.application.connection_operation(
            &identity,
            &action,
            source,
            crate::host_time::sample(),
            || Ok(config.expect("local launch")),
        ) {
            use crate::application::ConnectionEffect;
            let exit = matches!(effect, ConnectionEffect::Quit(_));
            match effect {
                ConnectionEffect::Quit(pending) => {
                    for (source, report) in pending {
                        renderer.show_operation_result(Some(source), report);
                    }
                }
                ConnectionEffect::Started => renderer.connection_attempt_started(),
                ConnectionEffect::Disconnected(disconnect) => {
                    renderer.pending_step_disconnect = Some(disconnect);
                    renderer.clear_connection();
                }
                _ => {}
            }
            return (exit, result);
        }
        if let Some(result) = renderer
            .application
            .policy_operation(&identity, &action, source)
        {
            return (false, result);
        }
        if let Some(result) = renderer.camera_operation(&identity, &action) {
            return (false, result);
        }
        if let Some((result, changed)) = renderer
            .application
            .settings
            .values
            .locale_operation(&identity, &action)
        {
            if let Some(locale) = changed {
                renderer.apply_property_value(client_commands::ClientPropertyValue::Locale(locale));
            }
            return (false, dispatch::native(result));
        }
        if let Some(result) = renderer.local_environment_operation(&identity, &action) {
            return (false, dispatch::native(result));
        }
        if let Some(result) = renderer.meteor_property_operation(&identity, &action) {
            return (false, dispatch::native(result));
        }
        if let Some(operation) =
            client_commands::meteor_adapter::MeteorOperation::from_action(&action)
        {
            if let Some(result) = renderer.meteor_operation(&identity, operation) {
                return (false, result);
            }
        }
        if let Some(operation) =
            client_commands::bolid_preview::BolidPreviewOperation::from_action(&action)
        {
            return (
                false,
                renderer.bolid_preview_operation(&identity, operation),
            );
        }
        if let Some(query) = client_commands::render_queries::RenderQuery::from_action(&action) {
            return (
                false,
                dispatch::native(renderer.render_query_operation(&identity, query)),
            );
        }
        if let client_commands::FrontendAction::Clock(clock) = &action {
            let frame = renderer
                .application
                .session()
                .map_or(renderer.environment_frame, |session| {
                    session.environment_frame()
                });
            return (
                false,
                renderer
                    .application
                    .clock_operation(identity, clock, frame, source),
            );
        }
        if let Some(result) = renderer.application.admin_help(&identity, &action, source) {
            return (false, result);
        }
        if let client_commands::FrontendAction::SpawnRandomBolid(count) = action {
            if renderer.pending_bolid_request.is_some() {
                return (
                    false,
                    dispatch::failure(
                        identity,
                        minimium_commands::results::Stage::Rejected,
                        "client_admin",
                        minimium_commands::CommandError::new(
                            minimium_commands::CommandErrorCode::Busy,
                            Some("server.sky.bolids.spawn"),
                            "a bolid request is waiting for a fresh gaze",
                            None::<String>,
                        ),
                    ),
                );
            }
            renderer.pending_bolid_request = Some(super::bolids::PendingBolidRequest {
                count,
                identity: identity.clone(),
                source,
            });
            renderer.last_sky_view_at = None;
            return (
                false,
                dispatch::native(minimium_commands::results::OperationResult::new(
                    identity,
                    minimium_commands::results::OperationReceipt::at(
                        minimium_commands::results::Stage::Submitted,
                    ),
                    Ok(minimium_commands::results::Payload::Value(
                        minimium_commands::operation::CommandValue::Unit,
                    )),
                )),
            );
        }
        if let Some(result) = renderer
            .application
            .admin_operation(&identity, &action, source)
        {
            return (false, result);
        }
        if let client_commands::FrontendAction::Print(text) = action {
            return (
                false,
                dispatch::native(minimium_commands::results::OperationResult::new(
                    identity,
                    minimium_commands::results::OperationReceipt::at(
                        minimium_commands::results::Stage::Completed,
                    ),
                    Ok(minimium_commands::results::Payload::Value(
                        minimium_commands::operation::CommandValue::Text(text),
                    )),
                )),
            );
        }
        (
            false,
            dispatch::failure(
                identity,
                minimium_commands::results::Stage::Rejected,
                "client_binding",
                minimium_commands::CommandError::new(
                    minimium_commands::CommandErrorCode::Unavailable,
                    Some("client operation"),
                    "catalog action has no desktop binding",
                    None::<String>,
                ),
            ),
        )
    })
}
