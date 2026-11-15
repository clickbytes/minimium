//! Bounded HTTP adapter for serving a validated, published material package.
use crate::materials::{self, MaterialPackage};
use minimium_assets::terrain_material::{digest, MaterialManifest, MAX_MANIFEST_BYTES};
use minimium_diagnostics::{DiagnosticEvent, DomainLogger, EventCode, Level};
use std::{
    collections::HashMap,
    fs,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const WORKERS: usize = 4;
const QUEUE: usize = 32;
const MAX_HEADER: usize = 8 * 1024;
const HEADER_DEADLINE: Duration = Duration::from_secs(5);
const IDLE_DEADLINE: Duration = Duration::from_secs(15);
const TRANSFER_DEADLINE: Duration = Duration::from_secs(300);

pub struct ContentServer {
    pub hash: [u8; 32],
    pub port: u16,
    stop: Arc<AtomicBool>,
    accept_thread: Option<JoinHandle<()>>,
    workers: Vec<JoinHandle<()>>,
    logger: DomainLogger,
}

impl ContentServer {
    pub fn start(
        root: &Path,
        bind_address: &str,
        port: u16,
        logger: DomainLogger,
    ) -> io::Result<Self> {
        let manifest_path = root.join("manifest.ron");
        let source = materials::read_bounded(&manifest_path, MAX_MANIFEST_BYTES as u64)
            .map_err(io::Error::other)?;
        let manifest = MaterialManifest::parse(&source).map_err(io::Error::other)?;
        for compressed in [true, false] {
            MaterialPackage::open(root, compressed).map_err(io::Error::other)?;
        }
        let mut files = HashMap::from([(
            "manifest.ron".to_owned(),
            (manifest_path, source.len() as u64),
        )]);
        for variant in &manifest.variants {
            for file in &variant.maps {
                files.insert(file.name.clone(), (root.join(&file.name), file.bytes));
            }
        }
        let hash = minimium_assets::terrain_material::decode_hash(&digest(&source))
            .map_err(io::Error::other)?;
        let files = Arc::new(files);
        let listener = TcpListener::bind((bind_address, port))?;
        listener.set_nonblocking(true)?;
        let bound_port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::sync_channel::<TcpStream>(QUEUE);
        let shared_receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(WORKERS);
        for worker_id in 0..WORKERS {
            let worker_receiver = shared_receiver.clone();
            let worker_stop = stop.clone();
            let worker_files = files.clone();
            let worker_logger = logger.clone();
            match thread::Builder::new()
                .name(format!("material-http-{worker_id}"))
                .spawn(move || {
                    worker_loop(worker_receiver, worker_files, worker_stop, worker_logger)
                }) {
                Ok(worker) => workers.push(worker),
                Err(error) => {
                    stop.store(true, Ordering::Release);
                    drop(sender);
                    wait_and_join(&mut workers, Duration::from_secs(2));
                    return Err(error);
                }
            }
        }
        let accept_stop = stop.clone();
        let accept_logger = logger.clone();
        let accept_thread = match thread::Builder::new()
            .name("material-http-listener".into())
            .spawn(move || accept_loop(listener, sender, accept_stop, accept_logger))
        {
            Ok(thread) => thread,
            Err(error) => {
                stop.store(true, Ordering::Release);
                wait_and_join(&mut workers, Duration::from_secs(2));
                return Err(error);
            }
        };
        Ok(Self {
            hash,
            port: bound_port,
            stop,
            accept_thread: Some(accept_thread),
            workers,
            logger,
        })
    }

    /// Stops accepting and waits only until `timeout`; returned names identify
    /// worker threads that exceeded the deadline.
    pub fn shutdown(&mut self, timeout: Duration) -> Vec<String> {
        self.stop.store(true, Ordering::Release);
        let deadline = Instant::now() + timeout;
        let mut unfinished = Vec::new();
        join_until(
            &mut self.accept_thread,
            deadline,
            "material-http-listener",
            &mut unfinished,
        );
        wait_and_join_until(&mut self.workers, deadline, &mut unfinished);
        for name in &unfinished {
            emit(
                &self.logger,
                Level::Warn,
                "asset_io.material_http_shutdown_timeout",
                "worker",
                name,
            );
        }
        unfinished
    }
}

impl Drop for ContentServer {
    fn drop(&mut self) {
        // Explicit shutdown owns the bounded wait and reports unfinished IDs.
        // Dropping after its deadline must not start another wait.
        self.stop.store(true, Ordering::Release);
    }
}

fn accept_loop(
    listener: TcpListener,
    sender: SyncSender<TcpStream>,
    stop: Arc<AtomicBool>,
    logger: DomainLogger,
) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                match sender.try_send(stream) {
                    Ok(()) => {}
                    Err(TrySendError::Full(mut stream)) => {
                        let _ = stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        emit(
                            &logger,
                            Level::Warn,
                            "asset_io.material_http_queue_full",
                            "status",
                            "503",
                        );
                    }
                    Err(TrySendError::Disconnected(_)) => break,
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20))
            }
            Err(error) => {
                emit(
                    &logger,
                    Level::Error,
                    "asset_io.material_http_accept_failed",
                    "reason",
                    &error.to_string(),
                );
                break;
            }
        }
    }
}

fn worker_loop(
    receiver: Arc<Mutex<Receiver<TcpStream>>>,
    files: Arc<HashMap<String, (PathBuf, u64)>>,
    stop: Arc<AtomicBool>,
    logger: DomainLogger,
) {
    loop {
        let stream = match receiver
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_millis(100))
        {
            Ok(stream) => stream,
            Err(RecvTimeoutError::Timeout) if !stop.load(Ordering::Acquire) => continue,
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return,
        };
        if stop.load(Ordering::Acquire) {
            return;
        }
        let mut stream = stream;
        match serve(&mut stream, &files, &stop) {
            Ok(status) => emit(
                &logger,
                Level::Info,
                "asset_io.material_http_response",
                "status",
                status,
            ),
            Err(error) => emit(
                &logger,
                Level::Warn,
                "asset_io.material_http_transfer_failed",
                "reason",
                &error.to_string(),
            ),
        }
    }
}

fn serve(
    stream: &mut TcpStream,
    files: &HashMap<String, (PathBuf, u64)>,
    stop: &AtomicBool,
) -> io::Result<&'static str> {
    stream.set_read_timeout(Some(Duration::from_millis(250)))?;
    stream.set_write_timeout(Some(IDLE_DEADLINE))?;
    let header_started = Instant::now();
    let mut header = Vec::with_capacity(1024);
    while !header.ends_with(b"\r\n\r\n") {
        if stop.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "server stopping",
            ));
        }
        if header.len() >= MAX_HEADER {
            write_error(stream, "431 Request Header Fields Too Large")?;
            return Ok("431");
        }
        if header_started.elapsed() >= HEADER_DEADLINE {
            write_error(stream, "408 Request Timeout")?;
            return Ok("408");
        }
        let mut byte = [0];
        match stream.read_exact(&mut byte) {
            Ok(()) => header.push(byte[0]),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                continue
            }
            Err(error) => return Err(error),
        }
    }
    let request = std::str::from_utf8(&header).map_err(io::Error::other)?;
    let request_path = request
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("GET /"))
        .and_then(|line| line.strip_suffix(" HTTP/1.1"));
    let Some((path, length)) = request_path.and_then(|name| files.get(name)) else {
        write_error(stream, "404 Not Found")?;
        return Ok("404");
    };
    let mut input = fs::File::open(Path::new(path))?;
    if input.metadata()?.len() != *length {
        return Err(io::Error::other("published material file changed"));
    }
    write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\nContent-Type: application/octet-stream\r\n\r\n")?;
    let transfer_started = Instant::now();
    let mut last_activity = Instant::now();
    let mut remaining = *length;
    let mut buffer = [0; 64 * 1024];
    while remaining > 0 {
        if stop.load(Ordering::Acquire) || transfer_started.elapsed() >= TRANSFER_DEADLINE {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "material transfer deadline",
            ));
        }
        if last_activity.elapsed() >= IDLE_DEADLINE {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "material transfer idle deadline",
            ));
        }
        let count = (remaining as usize).min(buffer.len());
        input.read_exact(&mut buffer[..count])?;
        stream.write_all(&buffer[..count])?;
        remaining -= count as u64;
        last_activity = Instant::now();
    }
    Ok("200")
}

fn write_error(stream: &mut TcpStream, status: &'static str) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

fn emit(logger: &DomainLogger, level: Level, code: &'static str, key: &str, value: &str) {
    let code = EventCode::new(code).expect("material HTTP event code is stable");
    let event: DiagnosticEvent = logger.event(level, code).field(key, value);
    logger.emit(event);
}

fn join_until(
    handle: &mut Option<JoinHandle<()>>,
    deadline: Instant,
    name: &str,
    unfinished: &mut Vec<String>,
) {
    let Some(thread) = handle.take() else {
        return;
    };
    while !thread.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if thread.is_finished() {
        let _ = thread.join();
    } else {
        unfinished.push(name.to_owned());
    }
}

fn wait_and_join(workers: &mut Vec<JoinHandle<()>>, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    let mut unfinished = Vec::new();
    wait_and_join_until(workers, deadline, &mut unfinished);
}

fn wait_and_join_until(
    workers: &mut Vec<JoinHandle<()>>,
    deadline: Instant,
    unfinished: &mut Vec<String>,
) {
    for worker in workers.drain(..) {
        while !worker.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        if worker.is_finished() {
            let _ = worker.join();
        } else {
            unfinished.push(
                worker
                    .thread()
                    .name()
                    .unwrap_or("material-http-worker")
                    .to_owned(),
            );
        }
    }
}
