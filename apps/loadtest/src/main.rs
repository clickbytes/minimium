use minimium_protocol::{decode_server, encode_client, ClientMessage, ServerMessage};
use std::{
    env,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Barrier,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("loadtest: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut address = None;
    let mut clients = 100usize;
    let mut seconds = 10u64;
    let args: Vec<String> = env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--address" => {
                index += 1;
                address = Some(
                    args.get(index)
                        .ok_or("--address requires HOST:PORT")?
                        .clone(),
                );
            }
            "--clients" => {
                index += 1;
                clients = args
                    .get(index)
                    .ok_or("--clients requires a number")?
                    .parse()?;
            }
            "--seconds" => {
                index += 1;
                seconds = args
                    .get(index)
                    .ok_or("--seconds requires a number")?
                    .parse()?;
            }
            "--help" | "-h" => {
                println!("minimium-loadtest --address HOST:PORT [--clients N] [--seconds N]");
                return Ok(());
            }
            other => return Err(format!("unknown option: {other}").into()),
        }
        index += 1;
    }
    let address = address.ok_or("--address is required")?;
    if clients == 0 || clients > 10_000 {
        return Err("clients must be between 1 and 10000".into());
    }
    let socket = address.to_socket_addrs()?.next().ok_or("invalid address")?;
    let process_salt = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_nanos()
        .to_le_bytes();
    let barrier = Arc::new(Barrier::new(clients + 1));
    let connected = Arc::new(AtomicU64::new(0));
    let pongs = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let mut workers = Vec::with_capacity(clients);
    for client_id in 0..clients {
        let barrier = Arc::clone(&barrier);
        let connected = Arc::clone(&connected);
        let pongs = Arc::clone(&pongs);
        let errors = Arc::clone(&errors);
        workers.push(thread::spawn(move || {
            let result = TcpStream::connect_timeout(&socket, Duration::from_secs(5));
            let Ok(mut stream) = result else {
                errors.fetch_add(1, Ordering::Relaxed);
                barrier.wait();
                return;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
            let _ = stream.set_nodelay(true);
            connected.fetch_add(1, Ordering::Relaxed);
            let mut profile_id = [0u8; 16];
            profile_id[..8].copy_from_slice(&(client_id as u64).to_le_bytes());
            profile_id[8..].copy_from_slice(&process_salt[..8]);
            let identity_frame = encode_client(&ClientMessage::Identify(profile_id));
            let name_frame = encode_client(&ClientMessage::SetName(format!("Bot{client_id}")));
            let ping_frame = encode_client(&ClientMessage::Ping);
            let (Ok(identity_frame), Ok(name_frame), Ok(ping_frame)) =
                (identity_frame, name_frame, ping_frame)
            else {
                errors.fetch_add(1, Ordering::Relaxed);
                barrier.wait();
                return;
            };
            if stream.write_all(&identity_frame).is_err() || stream.write_all(&name_frame).is_err()
            {
                errors.fetch_add(1, Ordering::Relaxed);
                barrier.wait();
                return;
            }
            barrier.wait();
            let until = Instant::now() + Duration::from_secs(seconds);
            let mut input = [0u8; 1024];
            let mut receive_buffer = Vec::with_capacity(4096);
            while Instant::now() < until {
                if stream.write_all(&ping_frame).is_err() {
                    errors.fetch_add(1, Ordering::Relaxed);
                    break;
                }
                match stream.read(&mut input) {
                    Ok(count) if count > 0 => {
                        receive_buffer.extend_from_slice(&input[..count]);
                        if receive_buffer.len() > 64 * 1024 {
                            errors.fetch_add(1, Ordering::Relaxed);
                            return;
                        }
                        loop {
                            match decode_server(&receive_buffer) {
                                Ok(Some((message, frame_bytes))) => {
                                    receive_buffer.drain(..frame_bytes);
                                    if matches!(message, ServerMessage::Pong) {
                                        pongs.fetch_add(1, Ordering::Relaxed);
                                    }
                                }
                                Ok(None) => break,
                                Err(_) => {
                                    errors.fetch_add(1, Ordering::Relaxed);
                                    return;
                                }
                            }
                        }
                    }
                    Ok(_) => break,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            || error.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        break;
                    }
                }
            }
        }));
    }
    let start = Instant::now();
    barrier.wait();
    for worker in workers {
        let _ = worker.join();
    }
    let elapsed = start.elapsed().as_secs_f64();
    println!("scenario=ping clients={clients} connected={} pongs={} errors={} seconds={elapsed:.2} pongs_per_second={:.1}", connected.load(Ordering::Relaxed), pongs.load(Ordering::Relaxed), errors.load(Ordering::Relaxed), pongs.load(Ordering::Relaxed) as f64 / elapsed.max(0.001));
    Ok(())
}
