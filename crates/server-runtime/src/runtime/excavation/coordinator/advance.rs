use super::*;
use crate::runtime::excavation::ports::Publication;

impl ExcavationCoordinator {
    pub(in crate::runtime::excavation) fn advance(&mut self, port: &mut Publication<'_>) {
        if let Some(preparation) = self.preparing.take() {
            match preparation.receiver.try_recv() {
                Ok(result) => self.finish_preparation(preparation, result, port),
                Err(std::sync::mpsc::TryRecvError::Empty) => self.preparing = Some(preparation),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    port.reject(preparation.request, ActionRejection::Capacity);
                }
            }
        }
        while let Some(result) = self.journal.poll_acknowledged() {
            let Some(pending) = self.pending.take() else {
                self.fail("unexpected durable excavation acknowledgement".into());
                break;
            };
            match result {
                Ok(ack)
                    if ack.plan.same_plan(&pending.prepared.mutation)
                        && matches!(
                            ack.commitment,
                            minimium_gameplay::WorldChangeCommitment::Durable
                                | minimium_gameplay::WorldChangeCommitment::Volatile
                        ) =>
                {
                    if !port.checkpoints_match(pending.prepared.mutation.impact_bodies()) {
                        self.fail("stale or mismatched impact checkpoint acknowledgement".into());
                        port.impact_failure(
                            "stale or mismatched excavation impact acknowledgement",
                        );
                        port.request_shutdown();
                        break;
                    }
                    port.record_persistence(pending.started.elapsed());
                    if let Err(error) = port.publish(DurablePublication {
                        prepared: pending.prepared,
                        origin: pending.origin,
                    }) {
                        self.fail(error.to_string());
                    }
                }
                result => {
                    let reason = match result {
                        Err(error) => {
                            let detail = error.to_string();
                            let reason = self.storage_rejection(error);
                            if !pending.prepared.mutation.impact_bodies().is_empty() {
                                port.clear_pending_impacts(
                                    pending.prepared.mutation.impact_bodies(),
                                );
                                port.impact_failure(&format!(
                                    "impact mutation was rejected by durable storage: {detail}"
                                ));
                            }
                            reason
                        }
                        Ok(_) => {
                            self.fail("excavation acknowledgement identity mismatch".into());
                            if !pending.prepared.mutation.impact_bodies().is_empty() {
                                port.impact_failure(
                                    "impact mutation acknowledgement identity mismatch",
                                );
                                port.request_shutdown();
                            }
                            ActionRejection::Storage
                        }
                    };
                    port.reject_action(pending.origin, reason);
                }
            }
        }
    }

    fn finish_preparation(
        &mut self,
        preparation: Preparation,
        result: Result<Prepared, PreparationError>,
        port: &mut Publication<'_>,
    ) {
        port.record_prepare(preparation.started.elapsed());
        let result = match result {
            Err(PreparationError::Geometry(failure)) => {
                port.report_geometry(&preparation.request, failure);
                Err(ActionRejection::UnsafeGeometry)
            }
            Err(PreparationError::Rejected(reason)) => Err(reason),
            Ok(prepared) => Ok(prepared),
        };
        let result = result.and_then(|prepared| {
            if !port.is_current(&preparation.request) {
                return Err(ActionRejection::Stale);
            }
            self.journal
                .validate(&prepared.mutation)
                .map_err(|error| self.storage_rejection(error))?;
            self.submit(prepared, preparation.request.origin)
                .map_err(|error| self.storage_rejection(error))
        });
        if let Err(reason) = result {
            port.reject(preparation.request, reason);
        }
    }
}
