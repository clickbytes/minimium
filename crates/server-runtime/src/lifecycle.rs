use crate::{
    environment::EnvironmentClock,
    executor::Executor,
    sector_streaming::SectorGenerationPool,
    storage::{PersistencePort, PlayerStore},
    world_registry::WorldRegistry,
};
use minimium_environment::time::MonotonicMs;
use std::{
    io, thread,
    time::{Duration, Instant},
};

pub(crate) struct ShutdownContext<'a> {
    pub shutdown_request: &'a crate::composition::ShutdownRequest,
    pub transport: &'a mut dyn minimium_transport::TransportPort,
    pub player_store: &'a mut PlayerStore,
    pub persistence: &'a mut dyn PersistencePort,
    pub content: &'a mut dyn crate::composition::ContentServicePort,
    pub sector_workers: &'a mut SectorGenerationPool,
    pub executor: &'a mut Executor,
    pub environment_clock: &'a mut EnvironmentClock,
    pub server_started: Instant,
    pub worlds: &'a WorldRegistry,
    pub map_fingerprint: u64,
    pub process_lifecycle: Option<&'a mut dyn crate::composition::ProcessLifecyclePort>,
    pub logger: &'a minimium_diagnostics::DomainLogger,
    pub runtime_error: Option<io::Error>,
}

pub(crate) fn shutdown(mut context: ShutdownContext<'_>) -> io::Result<()> {
    // One process-wide 30-second deadline; the final two seconds belong to diagnostics.
    let log_deadline = context.shutdown_request.begin_shutdown();
    let deadline = log_deadline - Duration::from_secs(2);
    context.transport.stop_admission();
    context.transport.reject_admin_pending();
    drain_transport(context.transport, &mut context.runtime_error, deadline);
    context.transport.close_all();
    let content_deadline = (Instant::now() + Duration::from_secs(2)).min(deadline);
    let mut unfinished = context
        .content
        .shutdown(content_deadline.saturating_duration_since(Instant::now()));
    unfinished.extend(context.sector_workers.shutdown_until(deadline));
    unfinished.extend(context.executor.shutdown_until(deadline));
    let snapshot = capture_final_snapshot(
        context.server_started,
        context.environment_clock,
        context.worlds,
        context.map_fingerprint,
        context.player_store,
        context.logger,
        deadline,
    );
    let save_result =
        persist_final_snapshot(context.persistence, snapshot, deadline, context.logger);
    if !unfinished.is_empty() {
        crate::diagnostics::emit(
            context.logger,
            minimium_diagnostics::Level::Error,
            "server.shutdown_workers_unfinished",
            &[("workers", unfinished.join(","))],
        );
        context.runtime_error.get_or_insert_with(|| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "workers exceeded shutdown deadline: {}",
                    unfinished.join(",")
                ),
            )
        });
    }
    finish_shutdown(
        context.process_lifecycle,
        context.runtime_error,
        save_result,
        context.logger,
    )
}

fn drain_transport(
    transport: &mut dyn minimium_transport::TransportPort,
    runtime_error: &mut Option<io::Error>,
    deadline: Instant,
) {
    while transport.has_pending_admin_output() && Instant::now() < deadline {
        match transport.poll(Instant::now()) {
            Ok(events) => {
                for event in events {
                    if let minimium_transport::TransportEvent::Admin(
                        minimium_transport::AdminTransportEvent::Request { id, .. },
                    ) = event
                    {
                        let _ = transport.respond_admin(
                            id,
                            minimium_transport::AdminResponse {
                                success: false,
                                error_code: Some("server.stopping".to_owned()),
                                key: None,
                                text: "server stopping".to_owned(),
                                usage: None,
                            },
                        );
                    }
                }
            }
            Err(error) => {
                runtime_error.get_or_insert_with(|| io::Error::other(error));
                break;
            }
        }
        let _ = transport.flush(Instant::now());
        thread::sleep(Duration::from_millis(5));
    }
}

fn capture_final_snapshot(
    server_started: Instant,
    environment_clock: &mut EnvironmentClock,
    worlds: &WorldRegistry,
    map_fingerprint: u64,
    player_store: &mut PlayerStore,
    logger: &minimium_diagnostics::DomainLogger,
    deadline: Instant,
) -> io::Result<crate::storage::SaveSnapshot> {
    if Instant::now() >= deadline {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "final snapshot capture missed shutdown deadline",
        ));
    }
    let capture_at = Instant::now();
    let monotonic_ms = u64::try_from(
        capture_at
            .saturating_duration_since(server_started)
            .as_millis(),
    )
    .map(MonotonicMs::new)
    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "server monotonic time overflow"));
    let snapshot = monotonic_ms
        .and_then(|at| {
            environment_clock
                .capture_saved_state(at)
                .map_err(io::Error::other)
        })
        .and_then(|saved_clock| {
            let input = worlds.snapshot_for_save(map_fingerprint)?;
            player_store
                .save_snapshot(input, saved_clock)
                .map_err(io::Error::from)
        });
    if let Err(error) = &snapshot {
        crate::diagnostics::emit(
            logger,
            minimium_diagnostics::Level::Error,
            "server.shutdown_snapshot_failed",
            &[("error_kind", format!("{:?}", error.kind()))],
        );
    }
    if snapshot.is_ok() && Instant::now() >= deadline {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "final snapshot capture exceeded shutdown deadline",
        ));
    }
    snapshot
}

fn persist_final_snapshot(
    persistence: &mut dyn PersistencePort,
    snapshot: io::Result<crate::storage::SaveSnapshot>,
    deadline: Instant,
    logger: &minimium_diagnostics::DomainLogger,
) -> io::Result<()> {
    match snapshot {
        Ok(snapshot) => persistence
            .shutdown_until(Some(snapshot), deadline)
            .map_err(io::Error::from),
        Err(error) => {
            if let Err(cleanup_error) = persistence.shutdown_until(None, deadline) {
                crate::diagnostics::emit(
                    logger,
                    minimium_diagnostics::Level::Error,
                    "server.shutdown_storage_failed",
                    &[("error_code", cleanup_error.code().to_owned())],
                );
            }
            Err(error)
        }
    }
}

fn finish_shutdown(
    mut process_lifecycle: Option<&mut dyn crate::composition::ProcessLifecyclePort>,
    runtime_error: Option<io::Error>,
    save_result: io::Result<()>,
    logger: &minimium_diagnostics::DomainLogger,
) -> io::Result<()> {
    let endpoint_result = process_lifecycle
        .as_deref_mut()
        .map_or(Ok(()), |process| process.remove_endpoint());
    let mut primary_error = runtime_error;
    for (operation, result) in [
        ("final save", save_result),
        ("endpoint cleanup", endpoint_result),
    ] {
        if let Err(error) = result {
            if primary_error.is_some() {
                crate::diagnostics::emit(
                    logger,
                    minimium_diagnostics::Level::Error,
                    "server.shutdown_secondary_failure",
                    &[
                        ("operation", operation.to_owned()),
                        ("error_kind", format!("{:?}", error.kind())),
                    ],
                );
            } else {
                primary_error = Some(error);
            }
        }
    }
    let shutdown_result = primary_error.map_or(Ok(()), Err);
    let status_result = process_lifecycle.as_deref_mut().map_or(Ok(()), |process| {
        process.write_shutdown_status(&shutdown_result)
    });
    if let Err(error) = &status_result {
        if shutdown_result.is_err() {
            crate::diagnostics::emit(
                logger,
                minimium_diagnostics::Level::Error,
                "server.shutdown_status_failed",
                &[("error_kind", format!("{:?}", error.kind()))],
            );
        }
    }
    shutdown_result?;
    status_result?;
    println!("server stopped cleanly");
    Ok(())
}
