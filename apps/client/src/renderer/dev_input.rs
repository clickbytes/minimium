//! Desktop execution and formatting for typed local developer presentation commands.
use super::DesktopHost;
use crate::client_commands::{
    output as dispatch, CommandOutput, DeveloperOperation, PresentationResult,
};

mod bolid_path;

pub(super) fn execute(
    renderer: &mut DesktopHost,
    line: &str,
) -> (bool, Option<dispatch::ClientResult>) {
    super::controller::execute_input(
        renderer,
        line,
        crate::client_commands::CommandSource::DeveloperStdin,
    )
}

pub(super) fn format_output(renderer: &DesktopHost, result: Option<dispatch::ClientResult>) {
    let result = dispatch::display(
        result,
        &renderer.localizer,
        renderer.application.settings.locale,
    );
    for output in result.outputs {
        match output {
            CommandOutput::Line(line) => println!("{line}"),
            CommandOutput::UiMessage(message) | CommandOutput::UiError(message) => println!(
                "{}",
                renderer
                    .localizer
                    .format(renderer.application.settings.locale, &message)
            ),
            CommandOutput::ClientText { message, .. } => println!(
                "{}",
                message.localized(&renderer.localizer, renderer.application.settings.locale)
            ),
            CommandOutput::Presentation(PresentationResult::Output(lines)) => {
                for line in lines {
                    println!("{line}");
                }
            }
            CommandOutput::Presentation(PresentationResult::Unavailable(unavailable)) => {
                println!("dev: {}", unavailable.message());
            }
            CommandOutput::Clear => {}
        }
    }
}

pub(super) fn execute_developer(
    renderer: &mut DesktopHost,
    operation: DeveloperOperation,
) -> Vec<String> {
    let mut lines = Vec::new();
    match operation {
        DeveloperOperation::Clouds => lines.extend(renderer.cloud_targets()),
        DeveloperOperation::BolidTransmission(enabled) => {
            let mode = if enabled { "on" } else { "off" };
            if renderer
                .renderer
                .milky_way
                .set_transmission_diagnostic(&renderer.renderer.queue, enabled)
            {
                lines.push(format!(
                    "dev: transmission diagnostic={mode}; requires an active bolid; black=opaque, white=clear; resize resets it"
                ));
            } else {
                lines.push("dev: bolid layer unavailable".to_owned());
            }
        }
        DeveloperOperation::BolidPath(path) => {
            lines.extend(bolid_path::restore(renderer, path));
        }
        DeveloperOperation::BolidFragments => bolid_fragments(renderer, &mut lines),
        DeveloperOperation::BolidLighting => bolid_lighting(renderer, &mut lines),
        DeveloperOperation::BolidReplay {
            age_seconds,
            seed,
            event_id,
            frozen,
        } => replay_bolid(renderer, age_seconds, seed, event_id, frozen, &mut lines),
        DeveloperOperation::BolidStatus => bolid_status(renderer, &mut lines),
        DeveloperOperation::Goto {
            latitude,
            longitude,
        } => goto(renderer, latitude, longitude, &mut lines),
        DeveloperOperation::Look {
            yaw_degrees,
            pitch_degrees,
        } => {
            renderer.input.yaw = yaw_degrees.to_radians();
            renderer.input.pitch = pitch_degrees.to_radians();
            lines.push("dev: view updated".to_owned());
        }
    }
    lines
}

fn bolid_fragments(renderer: &DesktopHost, lines: &mut Vec<String>) {
    let view = renderer.render_environment_frame.render_view;
    let matrix = glam::Mat4::from_cols_array_2d(&view.view_projection);
    for (event_index, event) in renderer
        .prepared_bolids
        .events
        .iter()
        .take(renderer.prepared_bolids.count[0] as usize)
        .enumerate()
    {
        let mut heads: Vec<_> = std::iter::once(event.head_power)
            .chain(event.fragments.iter().copied())
            .enumerate()
            .filter(|(_, head)| head[3] > 1e-6)
            .collect();
        heads.sort_by(|a, b| b.1[3].total_cmp(&a.1[3]));
        let screen = |head: [f32; 4]| {
            let clip = matrix * glam::Vec3::new(head[0], head[1], head[2]).extend(1.0);
            (clip.w > 0.0 && clip.is_finite()).then(|| {
                glam::Vec2::new(clip.x, clip.y) / clip.w
                    * glam::Vec2::new(view.viewport[0] as f32, view.viewport[1] as f32)
                    * 0.5
            })
        };
        let brightest = heads.first().and_then(|(_, head)| screen(*head));
        lines.push(format!(
            "bolid frame {event_index} age={} emitting={}",
            event.start_age[3],
            heads.len()
        ));
        for (index, head) in heads.into_iter().take(6) {
            let point = glam::Vec3::new(head[0], head[1], head[2]);
            let tail = event.tails[index];
            let tail = glam::Vec3::new(tail[0], tail[1], tail[2]);
            let clip = matrix * point.extend(1.0);
            let kind = if event.optics[index][3] >= 1.0 {
                "grains"
            } else {
                "solid"
            };
            let separation_px = brightest.zip(screen(head)).map(|(a, b)| a.distance(b));
            lines.push(format!(
                "  emitter={index} kind={kind} power={} wake_world_m={} ndc={:?} separation_from_brightest_px={separation_px:?}",
                head[3], point.distance(tail), (clip.truncate() / clip.w).to_array()
            ));
        }
    }
}

fn bolid_lighting(renderer: &DesktopHost, lines: &mut Vec<String>) {
    let frame = &renderer.render_environment_frame;
    let gpu = &renderer.renderer;
    let physical = gpu.hdr_enabled && gpu.atmosphere_gpu.is_active();
    let base = frame.environment_uniform.ambient_rgb_exposure[3];
    let shadow = &renderer.prepared_shadow;
    if shadow.valid && shadow.uniform.flags[1] == 3 {
        lines.push(format!(
            "bolid prepared shadow weight={}",
            f32::from_bits(shadow.uniform.flags[2])
        ));
    }
    let night_gain = minimium_renderer::render_frame::view_exposure(
        frame.render_view,
        frame.environment_uniform.global_sun_direction,
        1.0,
    );
    lines.push(format!(
        "bolid lighting hdr={} physical={} base_exposure={} night_gain={} terrain_debug={}",
        gpu.hdr_enabled, physical, base, night_gain, gpu.terrain_gpu.uniform.debug[0]
    ));
    if let Some((position, color)) = super::bolids::strongest_bolid_light(
        &renderer.prepared_bolids,
        frame.render_view.camera_eye_world_m,
        gpu.atmosphere_gpu.optical_uniform(),
    ) {
        let incident = minimium_renderer::bolid_light::incident_luminance(
            position,
            color,
            frame.render_view.camera_eye_world_m,
            gpu.atmosphere_gpu.optical_uniform(),
        );
        lines.push(format!(
            "bolid lighting position_power={position:?} color={color:?} incident_luminance={incident} exposed_incident={}",
            incident * f64::from(base * night_gain)
        ));
    } else {
        lines.push("bolid lighting: no incident source".to_owned());
    }
}

fn replay_bolid(
    renderer: &mut DesktopHost,
    age: f32,
    seed: Option<u64>,
    event_id: Option<u64>,
    frozen: bool,
    lines: &mut Vec<String>,
) {
    let now = renderer
        .application
        .connection_view()
        .session()
        .and_then(|session| session.environment_frame().simulation_ms);
    let context = renderer.render_environment_frame.context;
    let Some((event_context, event, cached)) =
        renderer
            .bolids
            .iter_mut()
            .rev()
            .find(|(candidate_context, event, _)| {
                Some(*candidate_context) == context && event_id.is_none_or(|id| event.id == id)
            })
    else {
        lines
            .push("dev: no matching retained bolid in this world; use dev bolid status".to_owned());
        return;
    };
    let Some(start) = now.and_then(|time| time.checked_sub((age * 1000.0) as u64)) else {
        lines.push("dev: synchronized server time is too early for this replay age".to_owned());
        return;
    };
    event.start_simulation_ms = start;
    renderer.diagnostic_bolid_age = frozen.then_some((*event_context, event.id, age));
    if let Some(seed) = seed {
        event.seed = seed;
        *cached = None;
    }
    lines.push(format!(
        "dev: local replay id={} seed={} age={age} frozen={frozen}",
        event.id, event.seed
    ));
}

fn bolid_status(renderer: &DesktopHost, lines: &mut Vec<String>) {
    let context = renderer.render_environment_frame.context;
    for (_, event, model) in renderer
        .bolids
        .iter()
        .filter(|(event_context, _, _)| Some(*event_context) == context)
    {
        if let Some(model) = model {
            lines.push(bolid_path::format(event));
            lines.push(format!(
                "bolid {} seed={} kind={} dust={} {:?}: {:?}",
                event.id,
                event.seed,
                event.kind,
                event.kind == 2 && event.seed % 9 == 0,
                model.family(),
                model.diagnostics()
            ));
            let view = renderer.render_environment_frame.render_view;
            let emitters = model.sample_emitters(model.luminous_end() * 0.5);
            if let Some(emitter) = emitters.iter().max_by(|a, b| a.power.total_cmp(&b.power)) {
                let origin = glam::DVec3::from_array(view.render_origin_m);
                let clip = glam::Mat4::from_cols_array_2d(&view.view_projection)
                    * (emitter.position - origin).as_vec3().extend(1.0);
                lines.push(format!(
                    "  mid-flight ndc={:?} altitude={}m distance={}m",
                    (clip.truncate() / clip.w).to_array(),
                    emitter.position.length() - view.game_planet_radius_m,
                    (emitter.position - glam::DVec3::from_array(view.camera_eye_world_m)).length()
                ));
            }
        }
    }
}

fn goto(renderer: &mut DesktopHost, latitude: f32, longitude: f32, lines: &mut Vec<String>) {
    let latitude = latitude.to_radians();
    let longitude = longitude.to_radians();
    let direction = [
        latitude.cos() * longitude.cos(),
        latitude.sin(),
        latitude.cos() * longitude.sin(),
    ];
    match renderer.application.execute_action(
        crate::application::ClientAction::MapTeleport { direction },
        crate::host_time::sample(),
    ) {
        crate::application::LocalActionOutcome::Queued { .. } => {
            lines.push("dev: Earth move requested".to_owned());
        }
        crate::application::LocalActionOutcome::Rejected { error, .. } => {
            lines.push(format!("dev: Earth move rejected: {error:?}"));
        }
        crate::application::LocalActionOutcome::Suppressed(_) => {}
    }
}
