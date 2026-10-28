mod config;
mod report;
mod scenarios;
use config::{ParseOutcome, StartupConfig};
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
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = match config::parse_env()? {
        ParseOutcome::Help => {
            println!("{}", config::HELP);
            return Ok(());
        }
        ParseOutcome::Config(config) => config,
    };
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
        server_data_dir,
        ..
    } = config;
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
                eprintln!("server sample before unavailable: address match or endpoint continuity could not be verified; all server samples are unavailable");
            }
        }
    }
    let socket = address.to_socket_addrs()?.next().ok_or("invalid address")?;
    let process_salt = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_nanos()
        .to_le_bytes();
    let ping_frame = encode_client(&ClientMessage::Ping)?;
    let movement_frames = MOVEMENT_DIRECTIONS
        .map(|direction| {
            encode_client(&ClientMessage::Move {
                direction,
                jump: false,
            })
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let fire_frames = MOVEMENT_DIRECTIONS
        .map(|direction| {
            encode_client(&ClientMessage::Fire {
                direction: [direction[0], 0.0, direction[1]],
            })
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let chat_frame = encode_client(&ClientMessage::Chat(String::from("load test message")))?;
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
        let movement_frames = movement_frames.clone();
        let fire_frames = fire_frames.clone();
        let chat_frame = chat_frame.clone();
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
                    movement_frames,
                    fire_frames,
                    chat_frame,
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
                eprintln!("server sample at measurement start unavailable: address match or endpoint continuity could not be verified; all server samples are unavailable");
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
                eprintln!("server sample after unavailable: address match or endpoint continuity could not be verified; all server samples are unavailable");
            }
        }
    }
    if !metadata.validate_server_identity() {
        eprintln!("server report samples unavailable: endpoint identity consistency could not be verified across samples");
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
    Ok(())
}
