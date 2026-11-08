mod config;
mod report;
mod scenarios;
use config::{ParseOutcome, StartupConfig};
use minimium_commands::CommandError;
use minimium_protocol::{encode_client, ClientMessage};

use std::{
    net::ToSocketAddrs,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
mod client_setup;
mod stats;
mod worker;
use stats::{ClientMeasurement, WorkerStats};
use worker::{run_worker, WorkerConfig, MOVEMENT_DIRECTIONS, POLL_INTERVAL};

fn main() {
    if let Err(error) = run() {
        eprintln!("loadtest: {error}");
        let exit_code = if error.downcast_ref::<CommandError>().is_some() {
            2
        } else {
            1
        };
        std::process::exit(exit_code);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = match config::parse_env()? {
        ParseOutcome::Help => {
            println!("{}", config::help());
            return Ok(());
        }
        ParseOutcome::Config(config) => config,
    };
    let paths = minimium_platform::app_paths::AppPaths::resolve(
        config
            .server_data_dir
            .as_deref()
            .unwrap_or(std::path::Path::new(".")),
        std::path::Path::new("."),
        std::path::Path::new("."),
        std::path::Path::new("."),
        config.log_dir.as_deref(),
    )?;
    let mut diagnostics = minimium_telemetry::logging::DiagnosticsRuntime::start(&paths.log_dir)?;
    let logger = diagnostics.logger(minimium_diagnostics::DomainId::Platform);
    logger.emit(logger.event(
        minimium_diagnostics::Level::Info,
        minimium_diagnostics::EventCode::new("loadtest.starting")?,
    ));
    let resolved_server_data_dir = config
        .server_data_dir
        .as_ref()
        .map(|_| paths.data_dir.clone());
    let worker_count = config.worker_count();
    let thread_mode = config.thread_mode();
    let StartupConfig {
        address,
        clients,
        seconds,
        scenario,
        output_format,
        cpu_label,
        cpu_mhz,
        scenario_id,
        server_data_dir: _,
        log_dir: _,
        ..
    } = config;
    let server_data_dir = resolved_server_data_dir;
    let mut metadata =
        report::ReportMetadata::new(scenario_id.unwrap_or_else(|| scenario.name().to_owned()));
    metadata.cpu_label = cpu_label;
    metadata.cpu_mhz = cpu_mhz;
    metadata.requested_seconds = seconds;
    metadata.address = address.clone();
    metadata.thread_mode = thread_mode.to_owned();
    if let Some(path) = &server_data_dir {
        match report::capture_server(path, &address) {
            Ok(sample) => metadata.server_before = Some(sample),
            Err(_) => {
                metadata.reject_server_samples();
                emit_loadtest_event(
                    &logger,
                    "loadtest.server_sample_unavailable",
                    "before_measurement",
                );
            }
        }
    }
    let socket = address.to_socket_addrs()?.next().ok_or("invalid address")?;
    let process_salt = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_nanos()
        .to_le_bytes();
    let ping_frame = encode_client(&ClientMessage::Ping)?;
    let movement_directions = MOVEMENT_DIRECTIONS.to_vec();
    let (ready_sender, ready_receiver) = mpsc::channel();
    let abort_setup = Arc::new(AtomicBool::new(false));
    let setup_started_at = Instant::now();
    let mut workers = Vec::with_capacity(worker_count);
    let mut start_senders = Vec::with_capacity(worker_count);

    for worker_id in 0..worker_count {
        let client_ids = (worker_id..clients).step_by(worker_count).collect();
        let ready_sender = ready_sender.clone();
        let abort_setup = Arc::clone(&abort_setup);
        let (start_sender, start_receiver) = mpsc::channel();
        start_senders.push(start_sender);
        let ping_frame = ping_frame.clone();
        let movement_directions = movement_directions.clone();
        workers.push(thread::spawn(move || {
            run_worker(
                client_ids,
                WorkerConfig {
                    socket,
                    process_salt,
                    seconds,
                    scenario,
                    total_clients: clients,
                    ping_frame,
                    movement_directions,
                    ready_sender,
                    start_receiver,
                    abort_setup,
                },
            )
        }));
    }
    drop(ready_sender);

    let mut ready_workers = 0;
    let mut setup_failed = false;
    while ready_workers < worker_count {
        match ready_receiver.recv_timeout(POLL_INTERVAL) {
            Ok(()) => ready_workers += 1,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if workers.iter().any(thread::JoinHandle::is_finished) {
                    setup_failed = true;
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                setup_failed = true;
                break;
            }
        }
    }
    if workers.iter().any(thread::JoinHandle::is_finished) {
        setup_failed = true;
    }
    if setup_failed {
        abort_setup.store(true, Ordering::Release);
        drop(start_senders);
        let mut stats = WorkerStats::default();
        for worker in workers {
            match worker.join() {
                Ok(worker_stats) => stats.merge(worker_stats),
                Err(_) => stats.errors = stats.errors.saturating_add(1),
            }
        }
        return Err(format!(
            "load worker failed during setup (ready workers: {ready_workers}/{worker_count}, handshakes: {})",
            stats.connected
        )
        .into());
    }

    if let Some(path) = &server_data_dir {
        match report::capture_server(path, &address) {
            Ok(sample) => metadata.server_measurement_start = Some(sample),
            Err(_) => {
                metadata.reject_server_samples();
                emit_loadtest_event(
                    &logger,
                    "loadtest.server_sample_unavailable",
                    "measurement_start",
                );
            }
        }
    }
    if workers.iter().any(thread::JoinHandle::is_finished) {
        abort_setup.store(true, Ordering::Release);
        drop(start_senders);
        for worker in workers {
            let _ = worker.join();
        }
        return Err("load worker exited after setup and before measurement".into());
    }
    let start = Instant::now() + Duration::from_millis(100);
    metadata.setup_elapsed_seconds = Some(start.duration_since(setup_started_at).as_secs_f64());
    for sender in start_senders {
        if sender.send(start).is_err() {
            setup_failed = true;
        }
    }
    if setup_failed {
        abort_setup.store(true, Ordering::Release);
        for worker in workers {
            let _ = worker.join();
        }
        return Err("load worker exited before measurement start".into());
    }

    let mut stats = WorkerStats::default();
    for worker in workers {
        match worker.join() {
            Ok(worker_stats) => stats.merge(worker_stats),
            Err(_) => stats.errors = stats.errors.saturating_add(1),
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    if let Some(path) = &server_data_dir {
        match report::capture_server(path, &address) {
            Ok(sample) => metadata.server_after = Some(sample),
            Err(_) => {
                metadata.reject_server_samples();
                emit_loadtest_event(
                    &logger,
                    "loadtest.server_sample_unavailable",
                    "after_measurement",
                );
            }
        }
    }
    if !metadata.validate_server_identity() {
        emit_loadtest_event(&logger, "loadtest.server_identity_unverified", "report");
    }
    report::print_report(
        output_format,
        scenario.name(),
        clients,
        worker_count,
        elapsed,
        &stats,
        &metadata,
    );
    drop(logger);
    diagnostics.shutdown_until(std::time::Instant::now() + std::time::Duration::from_secs(2))?;
    Ok(())
}

fn emit_loadtest_event(
    logger: &minimium_diagnostics::DomainLogger,
    code: &'static str,
    phase: &str,
) {
    if let Ok(code) = minimium_diagnostics::EventCode::new(code) {
        logger.emit(
            logger
                .event(minimium_diagnostics::Level::Warn, code)
                .field("phase", phase),
        );
    }
}
