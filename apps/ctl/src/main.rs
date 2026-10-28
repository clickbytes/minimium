use minimium_core::{commands::ServerCommand, endpoint::ServerEndpoint};
use std::{
    env, fs,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("minimiumctl: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "minimiumctl [--data-dir PATH] <status|players list|metrics show|world save|stop|help>"
        );
        return Ok(());
    }
    let mut data_dir = PathBuf::from(".minimium");
    if let Some(index) = args.iter().position(|arg| arg == "--data-dir") {
        if index + 1 >= args.len() {
            return Err("--data-dir requires a path".into());
        }
        data_dir = PathBuf::from(args.remove(index + 1));
        args.remove(index);
    }
    let command = args.join(" ");
    ServerCommand::parse(&command)?;
    let endpoint = ServerEndpoint::read(&data_dir)?;
    let address = ("127.0.0.1", endpoint.admin_port)
        .to_socket_addrs()?
        .next()
        .ok_or("could not resolve local admin endpoint")?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    writeln!(stream, "AUTH {}", endpoint.token)?;
    writeln!(stream, "{command}")?;
    let mut response = String::new();
    let mut stream = stream;
    stream.read_to_string(&mut response)?;
    let response = response.trim();
    if let Some(message) = response.strip_prefix("OK ") {
        if command == "stop" && message == "stopping" {
            wait_for_server_stop(&data_dir, endpoint.admin_port)?;
            println!("server stopped cleanly");
        } else {
            println!("{message}");
        }
    } else if response == "OK" {
        println!("ok");
    } else if let Some(message) = response.strip_prefix("ERR ") {
        return Err(message.into());
    } else {
        return Err("server returned an invalid response".into());
    }
    Ok(())
}

fn wait_for_server_stop(
    data_dir: &Path,
    admin_port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    const STOP_TIMEOUT: Duration = Duration::from_secs(120);
    let deadline = Instant::now() + STOP_TIMEOUT;
    let endpoint_path = ServerEndpoint::file_path(data_dir);
    let status_path = data_dir.join("server.shutdown");
    let expected_port = admin_port.to_string();
    loop {
        match fs::read_to_string(&status_path) {
            Ok(status) => {
                let mut fields = status.trim_end().split('\t');
                let recorded_port = fields.next().unwrap_or_default();
                let result = fields.next().unwrap_or_default();
                if recorded_port == expected_port {
                    match result {
                        "OK" if fields.next().is_none() => return Ok(()),
                        "ERR" => {
                            return Err(format!(
                                "server stopped with an error: {}",
                                fields.next().unwrap_or("unknown shutdown error")
                            )
                            .into())
                        }
                        _ => return Err("server wrote an invalid shutdown status".into()),
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }

        if Instant::now() >= deadline {
            if endpoint_path.exists() {
                return Err("server did not finish shutting down within 120 seconds".into());
            }
            return Err(
                "server stopped without writing a shutdown status within 120 seconds".into(),
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}
