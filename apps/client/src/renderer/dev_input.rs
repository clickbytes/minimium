//! Local desktop diagnostics fed from bounded stdin only with --dev-stdin.
use super::Renderer;

mod bolid_path;

pub(super) fn execute(renderer: &mut Renderer, line: &str) -> bool {
    let line = line.trim();
    if line.is_empty() {
        return false;
    }
    println!("dev> {line}");
    if let Some(mode) = line.strip_prefix("dev bolid transmission ") {
        let enabled = match mode {
            "on" => 1_u32,
            "off" => 0_u32,
            _ => {
                println!("dev: usage: dev bolid transmission on|off");
                return false;
            }
        };
        if renderer
            .gpu
            .milky_way
            .set_transmission_diagnostic(&renderer.gpu.queue, enabled != 0)
        {
            println!(
                "dev: transmission diagnostic={mode}; requires an active bolid; black=opaque, white=clear; resize resets it"
            );
        } else {
            println!("dev: bolid layer unavailable");
        }
        return false;
    }
    if line == "dev clouds" {
        for cloud in renderer.cloud_targets() {
            println!("{cloud}");
        }
        return false;
    }
    if let Some(rest) = line.strip_prefix("dev bolid path ") {
        bolid_path::restore(renderer, rest);
        return false;
    }
    if line == "dev bolid fragments" {
        let view = renderer.render_environment_frame.render_view;
        let matrix = glam::Mat4::from_cols_array_2d(&view.view_projection);
        let origin = glam::DVec3::from_array(view.render_origin_m).as_vec3();
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
            println!(
                "bolid frame {event_index} age={} emitting={}",
                event.start_age[3],
                heads.len()
            );
            for (index, head) in heads.into_iter().take(6) {
                let point = glam::Vec3::new(head[0], head[1], head[2]);
                let tail = event.tails[index];
                let tail = glam::Vec3::new(tail[0], tail[1], tail[2]);
                let clip = matrix * (point - origin).extend(1.0);
                let ndc = clip.truncate() / clip.w;
                println!(
                    "  emitter={index} power={} wake_world_m={} ndc={:?}",
                    head[3],
                    point.distance(tail),
                    ndc.to_array()
                );
            }
        }
        return false;
    }
    if line == "dev bolid lighting" {
        let frame = &renderer.render_environment_frame;
        let gpu = &renderer.gpu;
        let physical = gpu.hdr_enabled && gpu.atmosphere_gpu.is_active();
        let base = frame.environment_uniform.ambient_rgb_exposure[3];
        let shadow = &renderer.prepared_shadow;
        if shadow.valid && shadow.uniform.flags[1] == 3 {
            println!(
                "bolid prepared shadow weight={}",
                f32::from_bits(shadow.uniform.flags[2])
            );
        }
        let night_gain = minimium_renderer::lighting::view_exposure(
            frame.render_view,
            frame.environment_uniform.global_sun_direction,
            1.0,
        );
        println!(
            "bolid lighting hdr={} physical={} base_exposure={} night_gain={} terrain_debug={}",
            gpu.hdr_enabled, physical, base, night_gain, gpu.terrain_gpu.uniform.debug[0]
        );
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
            println!(
                "bolid lighting position_power={position:?} color={color:?} incident_luminance={incident} exposed_incident={}",
                incident * f64::from(base * night_gain)
            );
        } else {
            println!("bolid lighting: no incident source");
        }
        return false;
    }
    if let Some(rest) = line.strip_prefix("dev bolid replay ") {
        replay_bolid(renderer, rest, false);
        return false;
    }
    if let Some(rest) = line.strip_prefix("dev bolid freeze ") {
        replay_bolid(renderer, rest, true);
        return false;
    }
    if line == "dev bolid status" {
        for (_, event, _, model) in &renderer.bolids {
            if let Some(model) = model {
                bolid_path::print(event);
                println!(
                    "bolid {} seed={} kind={} dust={} {:?}: {:?}",
                    event.id,
                    event.seed,
                    event.kind,
                    event.kind == 2 && event.seed % 9 == 0,
                    model.family(),
                    model.diagnostics()
                );
                let view = renderer.render_environment_frame.render_view;
                let emitters = model.sample_emitters(model.luminous_end() * 0.5);
                if let Some(emitter) = emitters.iter().max_by(|a, b| a.power.total_cmp(&b.power)) {
                    let origin = glam::DVec3::from_array(view.render_origin_m).as_vec3();
                    let clip = glam::Mat4::from_cols_array_2d(&view.view_projection)
                        * (emitter.position - origin).extend(1.0);
                    println!(
                        "  mid-flight ndc={:?} altitude={}m distance={}m",
                        (clip.truncate() / clip.w).to_array(),
                        emitter.position.as_dvec3().length() - view.game_planet_radius_m,
                        (emitter.position.as_dvec3()
                            - glam::DVec3::from_array(view.camera_eye_world_m))
                        .length()
                    );
                }
            }
        }
        return false;
    }
    if let Some(rest) = line.strip_prefix("dev goto ") {
        goto(renderer, rest);
        return false;
    }
    if let Some(rest) = line.strip_prefix("dev look ") {
        look(renderer, rest);
        return false;
    }
    if line.len() > 128 {
        println!("dev: command exceeds 128 bytes");
        return false;
    }
    let console = &mut renderer.application.ui.console;
    console.clear_lines();
    console.editor.set_text(line);
    let before = console.diagnostic_line_count();
    let exit = super::controller::execute_console_input(renderer);
    for output in renderer
        .application
        .ui
        .console
        .diagnostic_lines_since(before)
    {
        println!("{output}");
    }
    exit
}

// Local presentation diagnosis only: never sends or edits a server event.
fn replay_bolid(renderer: &mut Renderer, args: &str, frozen: bool) {
    let mut parts = args.split_whitespace();
    let age = parts.next().and_then(|s| s.parse::<f32>().ok());
    let seed = parts.next().map(str::parse::<u64>).transpose();
    let event_id = parts.next().map(str::parse::<u64>).transpose();
    let (Some(age), Ok(seed), Ok(event_id)) = (age, seed, event_id) else {
        println!("dev: usage: dev bolid replay|freeze AGE_SECONDS [SEED [EVENT_ID]]");
        return;
    };
    if parts.next().is_some() || !age.is_finite() || !(0.0..=120.0).contains(&age) {
        println!("dev: replay age must be 0..120 seconds");
        return;
    }
    let now = renderer
        .application
        .connection
        .session()
        .and_then(|s| s.server_monotonic_ms());
    let context = renderer.render_environment_frame.context;
    let Some((event_context, event, received, cached)) =
        renderer.bolids.iter_mut().rev().find(|(c, event, _, _)| {
            Some(*c) == context && event_id.is_none_or(|id| event.id == id)
        })
    else {
        println!("dev: no matching retained bolid in this world; use dev bolid status");
        return;
    };
    let Some(start) = now.and_then(|t| t.checked_sub((age * 1000.0) as u64)) else {
        println!("dev: synchronized server time is too early for this replay age");
        return;
    };
    event.start_monotonic_ms = start;
    renderer.diagnostic_bolid_age = frozen.then_some((*event_context, event.id, age));
    *received = std::time::Instant::now();
    if let Some(seed) = seed {
        event.seed = seed;
        *cached = None;
    }
    println!(
        "dev: local replay id={} seed={} age={age} frozen={frozen}",
        event.id, event.seed
    );
}

fn look(renderer: &mut Renderer, angles: &str) {
    let mut parts = angles.split_whitespace();
    let Some((yaw, pitch)) = parts.next().zip(parts.next()) else {
        println!("dev: usage: dev look YAW_DEG PITCH_DEG");
        return;
    };
    if parts.next().is_some() {
        println!("dev: usage: dev look YAW_DEG PITCH_DEG");
        return;
    }
    let (Ok(yaw), Ok(pitch)) = (yaw.parse::<f32>(), pitch.parse::<f32>()) else {
        println!("dev: invalid angles");
        return;
    };
    if !yaw.is_finite() || !pitch.is_finite() || !(-89.0..=89.0).contains(&pitch) {
        println!("dev: angles out of range");
        return;
    }
    renderer.input.yaw = yaw.to_radians();
    renderer.input.pitch = pitch.to_radians();
    println!("dev: view updated");
}

fn goto(renderer: &mut Renderer, coordinates: &str) {
    let mut parts = coordinates.split_whitespace();
    let Some((latitude, longitude)) = parts.next().zip(parts.next()) else {
        println!("dev: usage: dev goto LATITUDE LONGITUDE");
        return;
    };
    if parts.next().is_some() {
        println!("dev: usage: dev goto LATITUDE LONGITUDE");
        return;
    }
    let (Ok(latitude), Ok(longitude)) = (latitude.parse::<f32>(), longitude.parse::<f32>()) else {
        println!("dev: invalid coordinates");
        return;
    };
    if !latitude.is_finite()
        || !longitude.is_finite()
        || !(-90.0..=90.0).contains(&latitude)
        || !(-180.0..=180.0).contains(&longitude)
    {
        println!("dev: coordinates out of range");
        return;
    }
    if !renderer.is_earth_world() || renderer.application.map_teleport_pending() {
        println!("dev: Earth is unavailable or a move is pending");
        return;
    }
    let latitude = latitude.to_radians();
    let longitude = longitude.to_radians();
    let direction = [
        latitude.cos() * longitude.cos(),
        latitude.sin(),
        latitude.cos() * longitude.sin(),
    ];
    match renderer
        .application
        .connection
        .queue_world_input(minimium_protocol::ClientWorldInput::MapTeleport { direction })
    {
        Ok(()) => {
            renderer.application.arm_map_teleport();
            renderer.application.request_map_teleport();
            println!("dev: Earth move requested");
        }
        Err(error) => println!("dev: Earth move rejected: {error:?}"),
    }
}
