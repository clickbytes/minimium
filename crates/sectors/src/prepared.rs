use super::*;

impl<T> SectorManager<T> {
    /// Replaces cached geometry as one owner-lane publication. All serials are
    /// checked before mutation, and late workers retain obsolete tickets.
    /// Uncached sectors remain on demand; this never expands the cache.
    pub fn publish_prepared(
        &mut self,
        instance: WorldInstanceId,
        generation: GenerationRevision,
        content: WorldContentRevision,
        revision: SectorRevision,
        payloads: &BTreeMap<SectorKey, Arc<T>>,
    ) -> Result<Vec<SectorTicket>, RequestError> {
        let addresses: Vec<_> = self
            .entries
            .keys()
            .filter(|address| address.instance == instance && payloads.contains_key(&address.key))
            .copied()
            .collect();
        self.next_serial
            .checked_add(addresses.len() as u64)
            .ok_or(RequestError::TicketSpaceExhausted)?;
        let mut tickets = Vec::with_capacity(addresses.len());
        for address in addresses {
            let ticket = SectorTicket {
                address,
                generation,
                content,
                serial: self.next_serial,
            };
            self.next_serial += 1;
            self.entries.insert(
                address,
                SectorEntry {
                    ticket,
                    state: SectorState::Installed,
                    payload: Some(payloads[&address.key].clone()),
                    revision: Some(revision),
                    collision_ready: true,
                    dirty: false,
                    pins: 0,
                },
            );
            tickets.push(ticket);
        }
        self.pending.retain(|ticket| {
            self.entries
                .get(&ticket.address)
                .is_some_and(|entry| entry.ticket == *ticket)
        });
        Ok(tickets)
    }
}
