//! Typed projections shared by admin, host, terminal and console outputs.
use crate::runtime::{SaveJobLookup, SaveJobRecord, SaveJobState, SaveJobsStatus, SaveSource};
use minimium_commands::operation::CommandValue as V;

fn record(fields: impl IntoIterator<Item = (&'static str, V)>) -> V {
    V::Record(
        fields
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    )
}

fn source(source: SaveSource) -> &'static str {
    match source {
        SaveSource::Manual => "manual",
        SaveSource::Autosave => "autosave",
        SaveSource::Shutdown => "shutdown",
    }
}

pub(crate) fn project_job(job: &SaveJobRecord) -> V {
    let (state, durability, code, detail) = match &job.state {
        SaveJobState::Queued => ("queued", V::Unit, V::Unit, V::Unit),
        SaveJobState::Completed(durability) => (
            "completed",
            V::Enum(
                match durability {
                    crate::storage::SaveDurability::Durable => "durable",
                    crate::storage::SaveDurability::Volatile => "volatile",
                    crate::storage::SaveDurability::BackendDefined => "backend_defined",
                }
                .into(),
            ),
            V::Unit,
            V::Unit,
        ),
        SaveJobState::Failed { code, detail } => (
            "failed",
            V::Unit,
            V::Text(code.clone()),
            V::Text(detail.clone()),
        ),
    };
    record([
        ("job_id", V::U128(job.id.0)),
        ("source", V::Enum(source(job.source).into())),
        ("state", V::Enum(state.into())),
        ("durability", durability),
        ("snapshot_revision", V::U64(job.snapshot_revision)),
        ("committed_tick", V::U64(job.committed_tick)),
        ("journal_revision", V::U64(job.journal_revision)),
        ("scope", V::Enum("all_worlds".into())),
        ("world_count", V::U64(job.world_count as u64)),
        ("queued_at_monotonic_ms", V::U64(job.queued_at_ms)),
        (
            "finished_at_monotonic_ms",
            job.finished_at_ms.map_or(V::Unit, V::U64),
        ),
        ("code", code),
        ("detail", detail),
    ])
}

pub(crate) fn project_status(status: &SaveJobsStatus) -> V {
    let (lookup, job) = match &status.lookup {
        SaveJobLookup::Never => ("never", V::Unit),
        SaveJobLookup::Untracked => ("untracked", V::Unit),
        SaveJobLookup::Expired => ("expired", V::Unit),
        SaveJobLookup::Tracked(job) => ("tracked", project_job(job)),
    };
    record([
        ("source", V::Enum("runtime_job_ledger".into())),
        ("lookup", V::Enum(lookup.into())),
        ("job", job),
        (
            "active",
            status.active.as_ref().map_or(V::Unit, project_job),
        ),
        ("last", status.last.as_ref().map_or(V::Unit, project_job)),
    ])
}
