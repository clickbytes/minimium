use super::*;

pub(super) fn take_finished(active: &mut ActiveAttempt) -> Option<()> {
    match active.receiver.try_recv() {
        Ok(result) => {
            if active.local {
                finalize_discard(active, result);
                None
            } else {
                drop(result);
                Some(())
            }
        }
        Err(TryRecvError::Disconnected)
            if active.worker.as_ref().is_some_and(JoinHandle::is_finished) =>
        {
            if let Some(worker) = active.worker.take() {
                let _ = worker.join();
            }
            Some(())
        }
        Err(TryRecvError::Disconnected) => None,
        Err(TryRecvError::Empty) if active.worker.as_ref().is_some_and(JoinHandle::is_finished) => {
            if let Some(worker) = active.worker.take() {
                let _ = worker.join();
            }
            Some(())
        }
        Err(TryRecvError::Empty) => None,
    }
}

/// Returns a discarded local result to its worker so its guard is dropped
/// away from the polling thread.
pub(super) fn finalize_discard(active: &mut ActiveAttempt, result: WorkerResult) {
    if let Some(finalize) = active.finalize.take() {
        let _ = finalize.send(WorkerFinalization::Discard(result));
    }
}

pub(super) fn run_operation(
    operation: WorkerOperation,
    deadline: Instant,
) -> io::Result<ConnectedSocket> {
    if Instant::now() >= deadline {
        return Err(timed_out_error());
    }
    match operation {
        WorkerOperation::Remote(address) => Ok(ConnectedSocket {
            stream: connect_all(&address, deadline)?,
            local_server: None,
        }),
        WorkerOperation::Local(config) => {
            let (stream, local_server) = bootstrap::connect_or_start_until(&config, deadline)
                .map_err(|error| {
                    io::Error::other(format!("local server startup failed: {error}"))
                })?;
            Ok(ConnectedSocket {
                stream,
                local_server: Some(local_server),
            })
        }
    }
}

pub(super) fn connect_all(address: &str, deadline: Instant) -> io::Result<TcpStream> {
    if Instant::now() >= deadline {
        return Err(timed_out_error());
    }

    let addresses: Vec<SocketAddr> = address.to_socket_addrs()?.collect();
    if addresses.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            format!("no socket addresses resolved for {address:?}"),
        ));
    }

    let mut last_error = None;
    let mut attempted = 0usize;
    for socket in addresses {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(timed_out_error());
        }
        attempted += 1;
        match TcpStream::connect_timeout(&socket, remaining) {
            Ok(stream) if Instant::now() < deadline => return Ok(stream),
            Ok(_) => return Err(timed_out_error()),
            Err(_error) if Instant::now() >= deadline => return Err(timed_out_error()),
            Err(error) => {
                last_error = Some(io::Error::new(
                    error.kind(),
                    format!("connection to {socket} failed: {error}"),
                ));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            format!("no socket address could be attempted for {address:?} ({attempted} tried)"),
        )
    }))
}

pub(super) fn timed_out_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "connection deadline expired during DNS or socket connection",
    )
}
