//! World climate configuration and separate bounded HTTP material endpoint.
use minimium_assets::terrain_material::{digest, MaterialManifest, MAX_MANIFEST_BYTES};
use minimium_environment::climate::ClimateScenario;
use std::{
    collections::HashMap,
    fs,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub(crate) struct ContentServer {
    pub hash: [u8; 32],
    pub port: u16,
    pub climate: ClimateScenario,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ContentServer {
    pub fn new(config: &crate::config::ServerConfig) -> io::Result<Self> {
        let climate_path = config.data_dir.join("world.climate.ron");
        let saved = climate_path.exists();
        let source = if saved {
            if config.climate_scenario_path.is_some() {
                return Err(io::Error::other(
                    "--climate-scenario initializes a new world only",
                ));
            }
            minimium_asset_io::materials::read_bounded(&climate_path, 16 * 1024)
                .map_err(io::Error::other)?
        } else if let Some(path) = &config.climate_scenario_path {
            minimium_asset_io::materials::read_bounded(path, 16 * 1024).map_err(io::Error::other)?
        } else {
            ron::to_string(&ClimateScenario::default())
                .map_err(io::Error::other)?
                .into_bytes()
        };
        let mut climate: ClimateScenario =
            ron::from_str(std::str::from_utf8(&source).map_err(io::Error::other)?)
                .map_err(io::Error::other)?;
        let changed = config
            .climate_relief_influence
            .is_some_and(|v| v != climate.relief_influence);
        if let Some(value) = config.climate_relief_influence {
            climate.relief_influence = value;
        }
        climate.validate().map_err(io::Error::other)?;
        if !saved || changed {
            let source = ron::to_string(&climate).map_err(io::Error::other)?;
            minimium_platform::files::atomic_write_file(
                &config.data_dir.join("world.climate.tmp"),
                &climate_path,
                source.as_bytes(),
            )?;
        }
        let mut server = Self {
            hash: [0; 32],
            port: 0,
            climate,
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
        };
        let Some(root) = &config.material_package_path else {
            return Ok(server);
        };
        let source = minimium_asset_io::materials::read_bounded(
            &root.join("manifest.ron"),
            MAX_MANIFEST_BYTES as u64,
        )
        .map_err(io::Error::other)?;
        let manifest = MaterialManifest::parse(&source).map_err(io::Error::other)?;
        // Validate both upload variants before advertising the required package.
        for compressed in [true, false] {
            minimium_asset_io::materials::MaterialPackage::open(root, compressed)
                .map_err(io::Error::other)?;
        }
        server.hash = minimium_assets::terrain_material::decode_hash(&digest(&source))
            .map_err(io::Error::other)?;
        let mut files = HashMap::from([(
            "manifest.ron".to_owned(),
            (root.join("manifest.ron"), source.len() as u64),
        )]);
        for variant in &manifest.variants {
            for file in &variant.maps {
                files.insert(file.name.clone(), (root.join(&file.name), file.bytes));
            }
        }
        let listener =
            TcpListener::bind((config.bind_address.as_str(), config.material_http_port))?;
        listener.set_nonblocking(true)?;
        server.port = listener.local_addr()?.port();
        let stop = server.stop.clone();
        server.thread = Some(
            std::thread::Builder::new()
                .name("material-http".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        match listener.accept() {
                            Ok((mut stream, _)) => {
                                let _ = serve(&mut stream, &files, &stop);
                            }
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(20))
                            }
                            Err(_) => break,
                        }
                    }
                })?,
        );
        Ok(server)
    }
}

impl Drop for ContentServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(
    stream: &mut TcpStream,
    files: &HashMap<String, (PathBuf, u64)>,
    stop: &AtomicBool,
) -> io::Result<()> {
    let started = Instant::now();
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if stop.load(Ordering::Relaxed) || started.elapsed() > Duration::from_secs(2) {
            return Err(io::Error::other("HTTP request deadline"));
        }
        if header.len() >= 8192 {
            return Err(io::Error::other("HTTP header limit"));
        }
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        header.push(byte[0]);
    }
    let text = std::str::from_utf8(&header).map_err(io::Error::other)?;
    let line = text.lines().next().unwrap_or("");
    let name = line
        .strip_prefix("GET /")
        .and_then(|s| s.strip_suffix(" HTTP/1.1"));
    if let Some((path, length)) = name.and_then(|name| files.get(name)) {
        let mut input = fs::File::open(Path::new(path))?;
        if input.metadata()?.len() != *length {
            return Err(io::Error::other("material file changed"));
        }
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\nContent-Type: application/octet-stream\r\n\r\n")?;
        let mut remaining = *length;
        let mut buffer = [0; 64 * 1024];
        while remaining > 0 {
            if stop.load(Ordering::Relaxed) || started.elapsed() > Duration::from_secs(300) {
                return Err(io::Error::other("HTTP transfer cancelled or timed out"));
            }
            let count = (remaining as usize).min(buffer.len());
            input.read_exact(&mut buffer[..count])?;
            stream.write_all(&buffer[..count])?;
            remaining -= count as u64;
        }
    } else {
        stream.write_all(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;
    }
    Ok(())
}
