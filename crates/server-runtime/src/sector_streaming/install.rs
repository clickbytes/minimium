use super::SectorPayload;
use crate::world_registry::WorldRegistry;
use minimium_sectors::{InstallResult, SectorManager};
use minimium_world::world::SectorRevision;
use std::sync::Arc;

const MAX_INSTALLS_PER_TICK: usize = 32;

pub(super) fn install_pending(
    worlds: &WorldRegistry,
    sectors: &mut SectorManager<SectorPayload>,
    geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
) {
    for _ in 0..MAX_INSTALLS_PER_TICK {
        let Some(ticket) = sectors.begin_next() else {
            break;
        };
        let Some(instance) = worlds.get(ticket.instance()) else {
            continue;
        };
        let payload = Arc::new(minimium_procgen::materialize_world_sector_with_geography(
            &instance.map,
            instance.manifest,
            ticket.key(),
            geography.clone(),
        ));
        let revision = SectorRevision::new(1).expect("initial sector revision is nonzero");
        if sectors.install_data(ticket, revision, payload) == InstallResult::Installed {
            let _ = sectors.install_collision(ticket);
        }
    }
}
