//! Host adapter for the OS monotonic clock. Logical owners receive the sample.
use minimium_client_runtime::ClientTime;

pub(crate) fn sample() -> ClientTime {
    ClientTime::new(std::time::Instant::now())
}
