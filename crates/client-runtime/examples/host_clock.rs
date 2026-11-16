//! Public session driven by explicit host samples; compiled, never executed here.
#[path = "support/local_content_jobs.rs"]
mod local_content_jobs;
#[path = "support/local_transport.rs"]
mod local_transport;

use minimium_client_runtime::{
    ClientAuthentication, ClientSession, ClientTime, ClientTimeError, ContentJobsSession,
};
use std::{
    num::NonZeroU64,
    sync::Arc,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Only the host samples the OS clock. Controlled samples share its domain.
    let initial = ClientTime::new(Instant::now());
    let jobs = ContentJobsSession::new(
        NonZeroU64::new(1).unwrap(),
        Arc::new(local_content_jobs::LocalContentJobs::default()),
    );
    let mut session = ClientSession::new(
        Box::new(local_transport::LocalTransport::default()),
        ClientAuthentication::new("local", b"credential".to_vec())?,
        jobs,
        initial,
    )?;
    let mut events = Vec::new();
    session.poll(&mut events, initial)?;
    // Equality is valid; a later operation can use a controlled later sample.
    session.poll(&mut events, initial)?;
    let later = initial.checked_add(Duration::from_millis(1))?;
    session.poll(&mut events, later)?;
    let before = session.metrics();
    match session.poll(&mut events, initial) {
        Err(ClientTimeError::Backward) => {}
        result => return Err(format!("unexpected backwards-time result: {result:?}").into()),
    }
    if session.metrics() != before {
        return Err("backwards sample changed transport accounting".into());
    }
    match initial.checked_add(Duration::MAX) {
        Err(ClientTimeError::Overflow) => {}
        result => return Err(format!("unexpected deadline overflow result: {result:?}").into()),
    }
    session.close(later)?;
    Ok(())
}
