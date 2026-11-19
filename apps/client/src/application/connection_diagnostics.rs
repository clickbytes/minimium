use super::ClientApplication;
use crate::client_commands::{
    connection_adapter::{failure, ConnectionReport as Report},
    output::ClientResult,
    CommandSource, FrontendAction,
};
use minimium_client_runtime::{
    ClientStepDisconnect, ClientStepOutput, ClientTime, ConnectionEvent, LocalServerLaunchConfig,
    SessionEvent, SessionState,
};
use minimium_commands::{
    results::{OperationContext, Stage},
    CommandErrorCode as Code,
};

pub(crate) enum ConnectionEffect {
    None,
    Started,
    Disconnected(ClientStepDisconnect),
    Quit(Vec<(CommandSource, ClientResult)>),
}
type Pending = (OperationContext, CommandSource, u64);
#[derive(Default)]
pub(super) struct Requests {
    attempt: Option<Pending>,
    ping: Option<Pending>,
}

impl ClientApplication {
    pub(crate) fn replica_position(&self) -> Option<[f32; 3]> {
        self.core.replica_player().map(|s| s.position)
    }

    pub(crate) fn connection_operation(
        &mut self,
        id: &OperationContext,
        action: &FrontendAction,
        source: CommandSource,
        now: ClientTime,
        config: impl FnOnce() -> Result<LocalServerLaunchConfig, String>,
    ) -> Option<(ConnectionEffect, ClientResult)> {
        let reject = |code, reason: &str| {
            Some((
                ConnectionEffect::None,
                failure(id.clone(), Stage::Rejected, code, reason),
            ))
        };
        let mut effect = ConnectionEffect::None;
        let mut stage = Stage::Completed;
        let report = match action {
            FrontendAction::Connect(_) | FrontendAction::HostLocal(_) => {
                if self.connection_view().busy() || self.connection_requests.attempt.is_some() {
                    return reject(Code::Busy, "another connection is active");
                }
                let local = matches!(action, FrontendAction::HostLocal(_));
                let started = match action {
                    FrontendAction::Connect(endpoint) => self.start_remote(endpoint.clone(), now),
                    FrontendAction::HostLocal(lanes) => {
                        let mut config = match config() {
                            Ok(config) => config,
                            Err(reason) => return reject(Code::InvalidValue, &reason),
                        };
                        if let Some(lanes) = lanes {
                            if config
                                .settings
                                .insert("server.threads".into(), lanes.as_str().into())
                                .is_some()
                            {
                                return reject(
                                    Code::ConflictingOptions,
                                    "local threads already configured",
                                );
                            }
                        }
                        self.start_local(config, now)
                    }
                    _ => unreachable!(),
                };
                if let Err(error) = started {
                    let code = match error {
                        minimium_client_runtime::connector::StartError::InvalidEndpoint => {
                            Code::InvalidEndpoint
                        }
                        minimium_client_runtime::connector::StartError::Busy { .. } => Code::Busy,
                        _ => Code::Unavailable,
                    };
                    return reject(code, &error.to_string());
                }
                let attempt = self
                    .connection_view()
                    .active_attempt_id()
                    .expect("started attempt");
                self.connection_requests.attempt = Some((id.clone(), source, attempt));
                stage = Stage::Submitted;
                effect = ConnectionEffect::Started;
                Report::Attempt { id: attempt, local }
            }
            FrontendAction::Disconnect => {
                let c = self.connection_view();
                let attempt = c.active_attempt_id();
                let session = c.session().map(|s| s.content_jobs_session_id());
                let owned = c.retained_owned_process();
                let cleanup = c.cleanup_pending();
                let previous_context = self.core.session_context();
                let previous_instance = self.core.view().world_instance;
                if let Err(e) = self.cancel_connection_attempt(now) {
                    return reject(Code::Unavailable, &e.to_string());
                }
                match self.disconnect_session(now) {
                    Ok(session) => {
                        effect = ConnectionEffect::Disconnected(ClientStepDisconnect {
                            session,
                            previous_instance,
                            previous_context,
                        })
                    }
                    Err(e) => return reject(Code::Unavailable, &e.to_string()),
                }
                Report::Disconnect {
                    cancelled_attempt: attempt,
                    closed_session: session,
                    owned_process_retained: owned,
                    cleanup_pending: cleanup,
                }
            }
            FrontendAction::NetStats => Report::Stats(self.connection_view().traffic(now)),
            FrontendAction::State => {
                let live = self.session().is_some_and(|s| {
                    s.state() == SessionState::Connected
                        && s.active_world_context().is_some()
                        && s.active_world_context() == self.core.session_context()
                });
                let v = self.core.view();
                Report::State {
                    predicted: live.then_some(v.player_position_m),
                    replica: self.core.replica_player().filter(|_| live),
                    health: live.then_some(v.health),
                    now,
                }
            }
            FrontendAction::Ping => {
                let Some(session) = self
                    .session()
                    .filter(|s| s.state() == SessionState::Connected)
                else {
                    return reject(Code::NoSession, "connected session required");
                };
                let session_id = session.content_jobs_session_id();
                if self.connection_requests.ping.is_some() || session.ping_pending() {
                    return reject(Code::Busy, "ping already pending");
                }
                match self.core.execute_action(super::ClientAction::Ping, now) {
                    super::LocalActionOutcome::Queued { .. } => {}
                    super::LocalActionOutcome::Rejected { error, .. } => {
                        return reject(
                            if matches!(error, minimium_client_runtime::QueueError::BufferFull) {
                                Code::QueueFull
                            } else {
                                Code::Unavailable
                            },
                            &error.to_string(),
                        )
                    }
                    _ => return reject(Code::Unavailable, "ping suppressed"),
                }
                self.connection_requests.ping = Some((id.clone(), source, session_id));
                stage = Stage::Submitted;
                Report::PingQueued {
                    session: session_id,
                }
            }
            FrontendAction::Quit => {
                let pending = [
                    self.connection_requests.attempt.take(),
                    self.connection_requests.ping.take(),
                ]
                .into_iter()
                .flatten()
                .map(|(request, source, _)| {
                    (
                        source,
                        failure(
                            request,
                            Stage::UnknownAfterDisconnect,
                            Code::UnknownAfterDisconnect,
                            "shutdown before confirmation",
                        ),
                    )
                })
                .collect();
                let mut pending: Vec<_> = pending;
                pending.extend(self.end_player_requests());
                effect = ConnectionEffect::Quit(pending);
                stage = Stage::OwnerAccepted;
                Report::ShutdownRequested
            }
            _ => return None,
        };
        Some((effect, report.result(id.clone(), stage)))
    }

    pub(crate) fn connection_receipts(
        &mut self,
        step: &ClientStepOutput,
    ) -> Vec<(CommandSource, ClientResult)> {
        let mut results = Vec::new();
        if let Some((id, source, attempt)) = &self.connection_requests.attempt {
            let active = self.connection_view().active_attempt_id() == Some(*attempt);
            let session = self
                .session()
                .filter(|s| s.content_jobs_session_id() == *attempt);
            let result = match (&step.connection, session) {
                (_, Some(s)) if s.state() == SessionState::Connected => Some(
                    Report::Connected {
                        id: *attempt,
                        session: s.content_jobs_session_id(),
                    }
                    .result(id.clone(), Stage::Completed),
                ),
                (ConnectionEvent::TimedOut, _) => Some(failure(
                    id.clone(),
                    Stage::Rejected,
                    Code::Timeout,
                    "connection attempt timed out",
                )),
                (ConnectionEvent::Failed(_) | ConnectionEvent::SetupFailed(_), _) => Some(failure(
                    id.clone(),
                    Stage::Rejected,
                    Code::Unavailable,
                    "connection attempt failed",
                )),
                (_, None)
                    if step.session_events.iter().any(|e| {
                        matches!(
                            e.event,
                            SessionEvent::Disconnected(
                                minimium_client_runtime::DisconnectReason::WelcomeTimeout
                            )
                        )
                    }) =>
                {
                    Some(failure(
                        id.clone(),
                        Stage::Rejected,
                        Code::Timeout,
                        "welcome timed out",
                    ))
                }
                (_, None) if step.disconnect.is_some() => Some(failure(
                    id.clone(),
                    Stage::Rejected,
                    Code::Unavailable,
                    "disconnected before welcome",
                )),
                (_, None) if !active => Some(failure(
                    id.clone(),
                    Stage::Cancelled,
                    Code::Unavailable,
                    "connection attempt ended before welcome",
                )),
                _ => None,
            };
            if let Some(result) = result {
                results.push((*source, result));
                self.connection_requests.attempt = None;
            }
        }
        if let Some((id, source, session)) = &self.connection_requests.ping {
            let same = self
                .session()
                .is_some_and(|s| s.content_jobs_session_id() == *session);
            let detached = step
                .disconnect
                .as_ref()
                .and_then(|d| d.session.as_ref())
                .is_some_and(|s| s.content_jobs_session_id() == *session);
            let pong = step.session_events.iter().find_map(|e| {
                if let SessionEvent::PingRoundTrip(rtt) = e.event {
                    Some(rtt)
                } else {
                    None
                }
            });
            let timeout = step.session_events.iter().any(|e| {
                matches!(
                    e.event,
                    SessionEvent::Disconnected(
                        minimium_client_runtime::DisconnectReason::HeartbeatTimeout
                    )
                )
            });
            let result = match pong {
                Some(rtt) if same || detached => Some(
                    Report::PingReply {
                        session: *session,
                        ms: rtt.as_millis(),
                    }
                    .result(id.clone(), Stage::Completed),
                ),
                _ if timeout => Some(failure(
                    id.clone(),
                    Stage::UnknownAfterDisconnect,
                    Code::Timeout,
                    "ping timed out",
                )),
                _ if !same => Some(failure(
                    id.clone(),
                    Stage::UnknownAfterDisconnect,
                    Code::UnknownAfterDisconnect,
                    "disconnected before matching pong",
                )),
                _ => None,
            };
            if let Some(result) = result {
                results.push((*source, result));
                self.connection_requests.ping = None;
            }
        }
        results
    }
}
