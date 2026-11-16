use super::super::DesktopHost;
use crate::client_commands::{
    MeteorLightDebug, PresentationOperation, PresentationRequirement, PresentationResult,
    PresentationUnavailable, PresentationUnavailableReason,
};

pub(super) fn execute(
    renderer: &mut DesktopHost,
    operation: PresentationOperation,
) -> PresentationResult {
    if let Some(unavailable) = unavailable(renderer, &operation) {
        return PresentationResult::Unavailable(unavailable);
    }
    let mut lines = Vec::new();
    match operation {
        PresentationOperation::Developer(operation) => {
            lines.extend(super::dev_input::execute_developer(renderer, operation));
        }
        PresentationOperation::AtmosphereStatus => {
            lines.extend(renderer.renderer.atmosphere_gpu.status());
        }
        PresentationOperation::WeatherStatus => lines.extend(renderer.weather_status()),
        PresentationOperation::TerrainStatus => lines.extend(renderer.terrain_status()),
        PresentationOperation::SkyInfo => lines.extend(renderer.sky_info_lines()),
        PresentationOperation::SkyBodies => {
            let frame = &renderer.render_environment_frame;
            if frame.observed_bodies.is_empty() {
                lines.push(renderer.localizer.format(
                    renderer.application.settings.locale,
                    &minimium_localization::UiMessage::Notice(
                        minimium_localization::UiNotice::SkyBodiesUnavailable,
                    ),
                ));
            } else {
                for body in frame
                    .observed_bodies
                    .iter()
                    .take(minimium_environment::universe::MAX_CELESTIAL_BODIES + 2)
                {
                    let flux: f64 = if body.id == minimium_environment::universe::SKY_SUN_ID {
                        frame.local_sun.map_or(0.0, |local| {
                            f64::from(local.effective_linear_rgb.iter().sum::<f32>())
                        })
                    } else if body.id == minimium_environment::universe::SKY_MOON_ID {
                        frame.local_moon.map_or(0.0, |local| {
                            f64::from(local.effective_linear_rgb.iter().sum::<f32>())
                        })
                    } else {
                        body.emitted_linear_rgb
                            .iter()
                            .chain(body.reflected_linear_rgb.iter())
                            .chain(body.scattered_linear_rgb.iter())
                            .sum()
                    };
                    lines.push(format!(
                        "body {} {:?} flux={flux:.4e} visible={}",
                        body.id.get(),
                        body.kind,
                        body.visible
                    ));
                }
            }
        }
        PresentationOperation::MeteorStatus => {
            lines.extend(super::super::meteor_diagnostics::status(renderer));
            lines.push(super::super::meteor_diagnostics::optical_status(renderer));
            lines.extend(renderer.meteors.status_lines());
        }
        PresentationOperation::MeteorSpawn(preset, seed) => {
            lines.push(
                renderer
                    .meteors
                    .spawn(preset, seed)
                    .unwrap_or_else(|error| error),
            );
        }
        PresentationOperation::MeteorClear => {
            lines.push(format!(
                "meteors: cleared {} active event(s)",
                renderer.meteors.clear()
            ));
        }
        PresentationOperation::MeteorLight(mode) => {
            renderer.renderer.transient_enabled = mode != MeteorLightDebug::Off;
            renderer.renderer.transient_transmission = mode == MeteorLightDebug::Transmission;
            lines.push(format!("meteors: local light {mode:?}"));
        }
        PresentationOperation::MeteorDebug(mode) => {
            lines.extend(super::super::meteor_diagnostics::debug(renderer, mode));
        }
    }
    PresentationResult::Output(lines)
}

fn unavailable(
    renderer: &DesktopHost,
    operation: &PresentationOperation,
) -> Option<PresentationUnavailable> {
    unavailable_for(renderer, operation.capability(), operation.requirement())
}

pub(super) fn unavailable_for(
    renderer: &DesktopHost,
    capability: crate::client_commands::PresentationCapability,
    requirement: PresentationRequirement,
) -> Option<PresentationUnavailable> {
    let reason = match requirement {
        PresentationRequirement::HostOnly => return None,
        PresentationRequirement::ReadyPresentation => presentation_reason(renderer, true),
        PresentationRequirement::ActiveWorldPresentation => {
            active_world_reason(renderer).or_else(|| presentation_reason(renderer, false))
        }
        PresentationRequirement::ActiveEarthWorld => active_world_reason(renderer)
            .or_else(|| {
                (!renderer.is_earth_world()).then_some(PresentationUnavailableReason::WrongWorld)
            })
            .or_else(|| {
                (!renderer.application.core_view().sectors_ready)
                    .then_some(PresentationUnavailableReason::CpuReadinessPending)
            })
            .or_else(|| presentation_reason(renderer, false)),
    }?;
    Some(PresentationUnavailable {
        capability,
        requirement,
        reason,
    })
}

fn active_world_reason(renderer: &DesktopHost) -> Option<PresentationUnavailableReason> {
    if renderer.application.map_teleport_pending() {
        return Some(PresentationUnavailableReason::WorldTransferPending);
    }
    let Some(session) = renderer.application.connection_view().session() else {
        return Some(PresentationUnavailableReason::MissingSession);
    };
    if session.state() != minimium_client_runtime::SessionState::Connected {
        return Some(PresentationUnavailableReason::MissingSession);
    }
    let Some(active_context) = session.active_world_context() else {
        return Some(PresentationUnavailableReason::NoActiveWorld);
    };
    (renderer.application.core_view().context != Some(active_context))
        .then_some(PresentationUnavailableReason::WorldTransferPending)
}

fn presentation_reason(
    renderer: &DesktopHost,
    allow_offline_demo: bool,
) -> Option<PresentationUnavailableReason> {
    if renderer.application.map_teleport_pending() {
        Some(PresentationUnavailableReason::WorldTransferPending)
    } else if !renderer.renderer.scene_visible {
        Some(if renderer.application.is_loading() {
            PresentationUnavailableReason::CpuReadinessPending
        } else {
            PresentationUnavailableReason::PresentationPending
        })
    } else if !presentation_context_matches(renderer, allow_offline_demo) {
        Some(PresentationUnavailableReason::WorldTransferPending)
    } else {
        None
    }
}

fn presentation_context_matches(renderer: &DesktopHost, allow_offline_demo: bool) -> bool {
    let frame_context = renderer.render_environment_frame.context;
    let core = renderer.application.core_view();
    if let Some(session) = renderer.application.connection_view().session() {
        return session.state() == minimium_client_runtime::SessionState::Connected
            && session
                .active_world_context()
                .is_some_and(|active_context| {
                    core.context == Some(active_context) && frame_context == Some(active_context)
                });
    }
    if !allow_offline_demo
        || !renderer.application.settings.demo
        || renderer.demo_environment.is_none()
        || renderer.application.connection_view().busy()
        || core.context.is_some()
    {
        return false;
    }
    let Some(manifest) = core.manifest else {
        return false;
    };
    let Some(epoch) = minimium_world::world::MembershipEpoch::new(1) else {
        return false;
    };
    let expected = minimium_protocol::WorldContext::new(
        core.world_instance,
        manifest.content_revision(),
        epoch,
    );
    renderer.environment_frame.quality == crate::environment::SynchronizationQuality::LocalDemo
        && renderer.environment_frame.context == Some(expected)
        && renderer.environment_frame.descriptor == renderer.render_environment_frame.descriptor
        && frame_context == Some(expected)
}
