use minimium_core::{admin_client, commands::ServerCommand, endpoint::ServerEndpoint};
use std::{env, path::PathBuf};

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
    if command == "stop" {
        admin_client::stop_and_wait(&data_dir, &endpoint)?;
        println!("server stopped cleanly");
    } else {
        let response = admin_client::request(&endpoint, &command)?;
        println!("{}", if response.is_empty() { "ok" } else { &response });
    }
    Ok(())
}
