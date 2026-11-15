//! Repeatable local presentation input; no server writes or injected events.
use super::DesktopHost;
use minimium_protocol::BolidEvent;

pub(super) fn format(event: &BolidEvent) -> String {
    let [x, y, z] = event.start_world_m;
    let [a, b, c] = event.end_world_m;
    format!(
        "dev bolid path {} {} {} {} {x} {y} {z} {a} {b} {c}",
        event.id, event.kind, event.seed, event.duration_ms
    )
}

pub(super) fn restore(
    renderer: &mut DesktopHost,
    path: crate::client_commands::BolidPath,
) -> Vec<String> {
    let mut output = Vec::new();
    let Some(start_monotonic_ms) = renderer
        .application
        .connection_view()
        .session()
        .and_then(|session| session.server_monotonic_ms())
    else {
        output.push("dev: synchronized server time is unavailable".to_owned());
        return output;
    };
    let candidate = BolidEvent {
        id: path.id,
        kind: path.kind,
        seed: path.seed,
        duration_ms: path.duration_ms,
        start_world_m: path.start_world_m,
        end_world_m: path.end_world_m,
        start_monotonic_ms,
    };
    let context = renderer.render_environment_frame.context;
    let Some((event_context, event, received, cached)) =
        renderer
            .bolids
            .iter_mut()
            .find(|(candidate_context, event, _, _)| {
                Some(*candidate_context) == context && event.id == candidate.id
            })
    else {
        output.push(
            "dev: no matching retained event in this world; spawn one and use its ID".to_owned(),
        );
        return output;
    };
    *event = candidate;
    *received = std::time::Instant::now();
    *cached = None;
    renderer.diagnostic_bolid_age = Some((*event_context, event.id, 0.0));
    output.push(
        "dev: restored local bolid path, frozen at age 0; use dev bolid freeze/replay".to_owned(),
    );
    output.push(format(event));
    output
}
