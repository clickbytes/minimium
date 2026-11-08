//! Repeatable local presentation input; no server writes or injected events.
use super::Renderer;
use minimium_protocol::BolidEvent;

pub(super) fn print(event: &BolidEvent) {
    let [x, y, z] = event.start_world_m;
    let [a, b, c] = event.end_world_m;
    println!(
        "dev bolid path {} {} {} {} {x} {y} {z} {a} {b} {c}",
        event.id, event.kind, event.seed, event.duration_ms
    );
}

pub(super) fn restore(renderer: &mut Renderer, args: &str) {
    let fields: Vec<_> = args.split_whitespace().collect();
    let parsed = (|| {
        if fields.len() != 10 {
            return None;
        }
        Some(BolidEvent {
            id: fields[0].parse().ok()?,
            kind: fields[1].parse().ok()?,
            seed: fields[2].parse().ok()?,
            duration_ms: fields[3].parse().ok()?,
            start_world_m: [
                fields[4].parse().ok()?,
                fields[5].parse().ok()?,
                fields[6].parse().ok()?,
            ],
            end_world_m: [
                fields[7].parse().ok()?,
                fields[8].parse().ok()?,
                fields[9].parse().ok()?,
            ],
            start_monotonic_ms: renderer
                .application
                .connection
                .session()?
                .server_monotonic_ms()?,
        })
    })();
    let Some(candidate) = parsed.filter(|event| event.valid()) else {
        println!(
            "dev: usage: dev bolid path ID KIND SEED DURATION_MS X0 Y0 Z0 X1 Y1 Z1; valid v19 values required"
        );
        return;
    };
    let context = renderer.render_environment_frame.context;
    let Some((event_context, event, received, cached)) = renderer
        .bolids
        .iter_mut()
        .find(|(c, event, _, _)| Some(*c) == context && event.id == candidate.id)
    else {
        println!("dev: no matching retained event in this world; spawn one and use its ID");
        return;
    };
    *event = candidate;
    *received = std::time::Instant::now();
    *cached = None;
    renderer.diagnostic_bolid_age = Some((*event_context, event.id, 0.0));
    println!("dev: restored local bolid path, frozen at age 0; use dev bolid freeze/replay");
    print(event);
}
