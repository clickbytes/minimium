//! Typed projections for authoritative bolid and impact command receipts.

use crate::hosting::replies::CommandPayloadView as View;
use minimium_commands::operation::CommandValue as V;

fn record<const N: usize>(fields: [(&str, V); N]) -> V {
    V::Record(
        fields
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    )
}

fn context(value: minimium_protocol::WorldContext) -> V {
    record([
        ("instance", V::U128(value.instance().get())),
        ("content_revision", V::U64(value.content_revision().get())),
        ("membership_epoch", V::U64(value.membership_epoch().get())),
    ])
}

pub(crate) fn project(view: &View<'_>) -> Option<V> {
    match view {
        View::Bolid {
            id,
            kind,
            recipients,
            shower,
            world,
            observer,
            context: selected,
            tick,
        } => Some(record([
            ("event_id", V::U64(*id)),
            ("shower_id", V::U64(*id)),
            (
                "requested",
                shower.map_or(V::U64(1), |count| V::U64(count as u64)),
            ),
            ("emitted", V::U64(1)),
            (
                "remaining",
                V::U64(shower.unwrap_or(1).saturating_sub(1) as u64),
            ),
            ("stage", V::Enum("owner_committed".into())),
            ("source", V::Enum("owner_snapshot".into())),
            ("world", V::U128(world.get())),
            ("observer", V::U64(observer.get())),
            ("context", context(*selected)),
            ("kind", V::U64(u64::from(*kind))),
            ("recipients", V::U64(*recipients as u64)),
            ("tick", V::U64(*tick)),
        ])),
        View::BolidStatus(status) => Some(record([
            ("shower_id", V::U64(status.id())),
            ("requested", V::U64(u64::from(status.requested()))),
            ("emitted", V::U64(status.emitted() as u64)),
            ("remaining", V::U64(status.remaining() as u64)),
            ("state", V::Enum(status.state().label().into())),
            (
                "reason",
                status
                    .reason()
                    .map_or(V::Unit, |reason| V::Text(reason.into())),
            ),
            ("source", V::Enum("owner_snapshot".into())),
            ("world", V::U128(status.world().get())),
            ("observer", V::U64(status.observer().get())),
            ("context", context(status.context())),
            (
                "event_ids",
                V::Page {
                    rows: status.event_ids().iter().copied().map(V::U64).collect(),
                    next_cursor: None,
                },
            ),
            ("started_tick", V::U64(status.started_tick())),
            (
                "terminal_tick",
                status.terminal_tick().map_or(V::Unit, V::U64),
            ),
        ])),
        View::ImpactSpawn {
            id,
            world,
            observer,
            context: selected,
            tick,
        } => Some(record([
            ("reserved_id", V::U64(*id)),
            ("checkpoint_pending", V::Bool(true)),
            ("stage", V::Enum("owner_accepted".into())),
            ("source", V::Enum("owner_snapshot".into())),
            ("world", V::U128(world.get())),
            ("observer", V::U64(observer.get())),
            ("context", context(*selected)),
            ("tick", V::U64(*tick)),
        ])),
        View::ImpactReceipt(receipt) => Some(record([
            ("reserved_id", V::U64(receipt.id())),
            ("world", V::U128(receipt.world().get())),
            ("observer", V::U64(receipt.observer().get())),
            ("context", context(receipt.context())),
            ("requested_tick", V::U64(receipt.requested_tick())),
            ("stage", V::Enum(receipt.stage().label().into())),
            (
                "checkpoint_pending",
                V::Bool(receipt.stage() == crate::runtime::ImpactReceiptStage::OwnerAccepted),
            ),
            (
                "journal_revision",
                receipt.journal_revision().map_or(V::Unit, V::U64),
            ),
            (
                "committed_tick",
                receipt.committed_tick().map_or(V::Unit, V::U64),
            ),
            (
                "reason",
                receipt
                    .reason()
                    .map_or(V::Unit, |reason| V::Text(reason.into())),
            ),
            ("source", V::Enum("owner_snapshot".into())),
        ])),
        _ => None,
    }
}
