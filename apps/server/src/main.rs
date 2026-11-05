#[cfg(feature = "allocation-metrics")]
use minimium_telemetry::allocations;
#[cfg(feature = "allocation-metrics")]
#[global_allocator]
static ALLOCATOR: allocations::CountingAllocator = allocations::CountingAllocator;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = minimium_server_runtime::run(&args) {
        eprintln!("server error: {error}");
        std::process::exit(1);
    }
}
