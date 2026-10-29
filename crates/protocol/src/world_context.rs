use minimium_core::world::{MembershipEpoch, WorldContentRevision, WorldInstanceId};

/// Identifies the authoritative membership scope for every world input and snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldContext {
    instance: WorldInstanceId,
    content_revision: WorldContentRevision,
    membership_epoch: MembershipEpoch,
}

impl WorldContext {
    pub const fn new(
        instance: WorldInstanceId,
        content_revision: WorldContentRevision,
        membership_epoch: MembershipEpoch,
    ) -> Self {
        Self {
            instance,
            content_revision,
            membership_epoch,
        }
    }

    pub const fn instance(self) -> WorldInstanceId {
        self.instance
    }

    pub const fn content_revision(self) -> WorldContentRevision {
        self.content_revision
    }

    pub const fn membership_epoch(self) -> MembershipEpoch {
        self.membership_epoch
    }
}
