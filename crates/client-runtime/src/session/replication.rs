use super::*;

impl ClientSession {
    pub fn register_replica_schema(
        &mut self,
        kind: ComponentTypeId,
        schema: SchemaVersion,
    ) -> Result<(), crate::ReplicaError> {
        if self
            .replica_schemas
            .get(&kind)
            .is_some_and(|registered| *registered != schema)
        {
            return Err(crate::ReplicaError::SchemaConflict);
        }
        self.replica_schemas.insert(kind, schema);
        Ok(())
    }

    pub fn replicated_entities(&self) -> Option<&crate::ReplicatedEntities> {
        let context = self.world_context?;
        (self.replicas.context == Some(context)).then_some(&self.replicas)
    }

    pub(super) fn apply_replication_message(
        &mut self,
        message: &ServerMessage,
    ) -> Result<bool, crate::ReplicaError> {
        use minimium_protocol::EntityReplicationOperation;
        match message {
            ServerMessage::EntitySyncBegin {
                context,
                sync_id,
                base_revision,
                target_revision,
                full,
            } => {
                if Some(*context) != self.world_context {
                    return Ok(false);
                }
                if *sync_id == 0
                    || target_revision < base_revision
                    || (*full && *base_revision != 0)
                {
                    return Err(crate::ReplicaError::InvalidSync);
                }
                if self
                    .replica_assembly
                    .as_ref()
                    .is_some_and(|pending| pending.sync_id == *sync_id || !*full)
                {
                    return Err(crate::ReplicaError::InvalidSync);
                }
                let (state, payload_bytes) = if *full {
                    let mut state = crate::ReplicatedEntities::default();
                    state.context = Some(*context);
                    (state, 0)
                } else {
                    if self.replicas.context != Some(*context)
                        || self.replicas.revision != *base_revision
                    {
                        return Err(crate::ReplicaError::UnexpectedRevision);
                    }
                    let bytes = self
                        .replicas
                        .entities
                        .values()
                        .flat_map(|entity| entity.components.values())
                        .map(|component| component.bytes.len())
                        .sum();
                    (self.replicas.clone(), bytes)
                };
                self.replica_assembly = Some(crate::replicas::ReplicaAssembly {
                    context: *context,
                    sync_id: *sync_id,
                    target_revision: *target_revision,
                    state,
                    payload_bytes,
                });
                Ok(true)
            }
            ServerMessage::EntitySyncRecord {
                context,
                sync_id,
                revision,
                operation,
            } => {
                if Some(*context) != self.world_context {
                    return Ok(false);
                }
                let assembly = self
                    .replica_assembly
                    .as_mut()
                    .ok_or(crate::ReplicaError::InvalidSync)?;
                if assembly.context != *context || assembly.sync_id != *sync_id {
                    return Err(crate::ReplicaError::InvalidSync);
                }
                if *revision != assembly.target_revision {
                    return Err(crate::ReplicaError::UnexpectedRevision);
                }
                if let EntityReplicationOperation::ComponentUpsert { bytes, .. } = operation {
                    if bytes.len() > minimium_world::entity::MAX_COMPONENT_BYTES {
                        return Err(crate::ReplicaError::ByteLimit);
                    }
                }
                assembly.state.apply(
                    operation,
                    &self.replica_schemas,
                    &mut assembly.payload_bytes,
                )?;
                Ok(true)
            }
            ServerMessage::EntitySyncComplete {
                context,
                sync_id,
                target_revision,
            } => {
                if Some(*context) != self.world_context {
                    return Ok(false);
                }
                let mut assembly = self
                    .replica_assembly
                    .take()
                    .ok_or(crate::ReplicaError::InvalidSync)?;
                if assembly.context != *context
                    || assembly.sync_id != *sync_id
                    || assembly.target_revision != *target_revision
                {
                    return Err(crate::ReplicaError::InvalidSync);
                }
                assembly.state.context = Some(*context);
                assembly.state.revision = *target_revision;
                self.replicas = assembly.state;
                self.queue(&ClientMessage::ReplicationAck {
                    context: *context,
                    sync_id: *sync_id,
                    revision: *target_revision,
                    accepted: true,
                })
                .map_err(|_| crate::ReplicaError::AcknowledgementQueueFull)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub(super) fn clear_replicas(&mut self) {
        self.replica_assembly = None;
        self.replicas = crate::ReplicatedEntities::default();
    }
}
