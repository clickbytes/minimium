use super::output::{ClientPayload, ClientResult};
use minimium_client_runtime::{ClientTime, ConnectionStats, ReplicaPlayerSample};
use minimium_commands::results::{
    OperationContext, OperationFailure, OperationReceipt, OperationResult, Stage,
};
use minimium_commands::{CommandError, CommandErrorCode};

pub(crate) enum ConnectionReport {
    Attempt {
        id: u64,
        local: bool,
    },
    Connected {
        id: u64,
        session: u64,
    },
    Disconnect {
        cancelled_attempt: Option<u64>,
        closed_session: Option<u64>,
        owned_process_retained: bool,
        cleanup_pending: bool,
    },
    PingQueued {
        session: u64,
    },
    PingReply {
        session: u64,
        ms: u128,
    },
    Stats(ConnectionStats),
    State {
        predicted: Option<[f64; 3]>,
        replica: Option<ReplicaPlayerSample>,
        now: ClientTime,
        health: Option<u16>,
    },
    ShutdownRequested,
}
impl ConnectionReport {
    pub(super) fn render(
        &self,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) -> String {
        use minimium_localization::{UiMessage, UiNotice};
        let notice = match self {
            Self::Stats(_) => UiNotice::ConnectionTraffic,
            Self::State { .. } => UiNotice::PlayerStateSources,
            _ => UiNotice::ConnectionReceipt,
        };
        let title = localizer.format(locale, &UiMessage::Notice(notice));
        format!("{title} {}", self.details())
    }
    fn details(&self) -> String {
        match self{
            Self::Attempt{id,local}=>format!("attempt={id} local={local}"),
            Self::Connected{id,session}=>format!("attempt={id} session={session}"),
            Self::Disconnect{cancelled_attempt,closed_session,owned_process_retained,cleanup_pending}=>format!("cancelled_attempt={cancelled_attempt:?} closed_session={closed_session:?} owned_process_retained={owned_process_retained} cleanup_pending={cleanup_pending}"),
            Self::PingQueued{session}=>format!("session={session} queued"),
            Self::PingReply{session,ms}=>format!("session={session} rtt_ms={ms}"),
            Self::Stats(s)=>{
                let sample=s.session.map_or("unavailable".into(),|s|format!("{} sent_bytes={} received_bytes={} ping_ms={:?} ping_age_ms={:?}",s.session,s.bytes_sent,s.bytes_received,s.ping_ms,s.ping_age_ms));
                format!("process sent_bytes={} received_bytes={} session={sample}",s.process_sent,s.process_received)
            }
            Self::State{predicted,replica,now,health}=>{
                let sample=replica.map_or("unavailable".into(),|r|format!("position_m={:?} server_tick={} server_ms={} age_ms={}",r.position,r.server_tick,r.server_time_ms,now.as_instant().saturating_duration_since(r.received_at.as_instant()).as_millis()));
                format!("predicted_m={predicted:?} replica={sample} health={health:?}")
            }
            Self::ShutdownRequested=>"shutdown_requested=true".into(),
        }
    }
    pub(crate) fn result(self, identity: OperationContext, stage: Stage) -> ClientResult {
        OperationResult::new(
            identity,
            OperationReceipt::at(stage),
            Ok(ClientPayload::Connection(self)),
        )
    }
}
pub(crate) fn failure(
    identity: OperationContext,
    stage: Stage,
    code: CommandErrorCode,
    reason: &str,
) -> ClientResult {
    OperationResult::new(
        identity,
        OperationReceipt::at(stage),
        Err(OperationFailure::command(
            "connection",
            CommandError::new(code, Some("connection"), reason, None::<String>),
        )),
    )
}
