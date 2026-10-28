//! Report data capture and serialization.

mod capture;
mod data;
mod encoding;
mod json;
mod text;

pub use capture::{capture_server, ReportMetadata};

use super::config::OutputFormat;
use super::WorkerStats;
use data::ReportData;

pub fn print_report(
    format: OutputFormat,
    scenario: &str,
    requested_clients: usize,
    workers: usize,
    elapsed_seconds: f64,
    stats: &WorkerStats,
    metadata: &ReportMetadata,
) {
    let data = ReportData::new(
        scenario,
        requested_clients,
        workers,
        elapsed_seconds,
        stats,
        metadata,
    );
    match format {
        OutputFormat::Json => println!("{}", json::render(&data)),
        OutputFormat::Text => println!("{}", text::render(&data)),
    }
}
