use super::super::Renderer;
use crate::client_commands::{MeteorLightDebug, PresentationOperation, PresentationResult};

pub(super) fn execute(
    renderer: &mut Renderer,
    operation: PresentationOperation,
) -> PresentationResult {
    let mut lines = Vec::new();
    match operation {
        PresentationOperation::Developer(operation) => {
            lines.extend(super::dev_input::execute_developer(renderer, operation));
        }
        PresentationOperation::AtmosphereStatus => {
            lines.extend(renderer.gpu.atmosphere_gpu.status());
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
            renderer.gpu.transient_enabled = mode != MeteorLightDebug::Off;
            renderer.gpu.transient_transmission = mode == MeteorLightDebug::Transmission;
            lines.push(format!("meteors: local light {mode:?}"));
        }
        PresentationOperation::MeteorDebug(mode) => {
            lines.extend(super::super::meteor_diagnostics::debug(renderer, mode));
        }
    }
    PresentationResult::Output(lines)
}
