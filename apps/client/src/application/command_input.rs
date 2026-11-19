use super::{ClientApplication, ClientTime, QueuedActionKind};

impl ClientApplication {
    pub(crate) fn execute_text_command(
        &mut self,
        input: Result<&str, minimium_commands::CommandError>,
        source: crate::client_commands::CommandSource,
        fire_direction: [f32; 3],
        player_position: Option<[f32; 3]>,
        now: ClientTime,
    ) -> crate::client_commands::output::Submission {
        let mut identity = minimium_commands::results::OperationContext {
            request_id: self.next_command_id,
            correlation_nonce: None,
            operation: minimium_commands::operation::OperationId("ui.console.input".into()),
            scope: "Client".into(),
            session: self.core.session().map(|s| s.content_jobs_session_id()),
            context: self
                .core
                .session()
                .and_then(|s| s.active_world_context())
                .map(|c| minimium_commands::results::WorldBinding {
                    instance: c.instance().get(),
                    content_revision: c.content_revision().get(),
                    membership_epoch: c.membership_epoch().get(),
                }),
        };
        let Some(next_id) = self.next_command_id.checked_add(1) else {
            let error = minimium_commands::CommandError::new(
                minimium_commands::CommandErrorCode::ResultLimit,
                Some("request_id"),
                "command request identifiers exhausted",
                None::<String>,
            );
            return (
                identity,
                crate::client_commands::TextCommandResult::Rejected(error),
            );
        };
        self.next_command_id = next_id;
        let input = match input {
            Ok(line) => line,
            Err(error) => {
                return (
                    identity,
                    crate::client_commands::TextCommandResult::Rejected(error),
                )
            }
        };
        if let Err(error) = self.core.validate_time(now) {
            return (
                identity,
                crate::client_commands::TextCommandResult::QueueRejected {
                    kind: QueuedActionKind::Other,
                    error: crate::session::QueueError::Time(error),
                },
            );
        }
        let portal_entity = player_position.and_then(|position| {
            self.core
                .session()
                .and_then(|session| session.replicated_entities())
                .and_then(|replicas| {
                    crate::client_commands::nearest_portal_entity(replicas, position)
                })
        });
        let mut context = crate::client_commands::CommandContext {
            portal_entity,
            fire_direction,
            player_position,
        };
        let result =
            crate::client_commands::execute_text(input, source, &mut context, &mut identity);
        (identity, result)
    }
}
