use super::ClientApplication;

impl ClientApplication {
    pub(crate) fn script_operation(
        &mut self,
        identity: minimium_commands::results::OperationContext,
        control: crate::client_commands::ScriptControl,
    ) -> crate::client_commands::output::ClientResult {
        #[cfg(feature = "console-scripting")]
        {
            return crate::client_commands::output::native(
                self.scripting.operate(identity, control),
            );
        }
        #[cfg(not(feature = "console-scripting"))]
        {
            let _ = control;
            crate::client_commands::output::failure(
                identity,
                minimium_commands::results::Stage::Rejected,
                "client_scripting",
                minimium_commands::CommandError::new(
                    minimium_commands::CommandErrorCode::Unavailable,
                    Some("ConsoleScriptingDisabled"),
                    "console scripting feature is disabled",
                    None::<String>,
                ),
            )
        }
    }

    #[cfg(feature = "console-scripting")]
    pub(crate) fn take_script_request(&mut self) -> Option<crate::scripting::HostRequest> {
        self.scripting.take_request()
    }

    #[cfg(feature = "console-scripting")]
    pub(crate) fn prepare_script_request(
        &mut self,
        request: &crate::scripting::HostRequest,
    ) -> crate::client_commands::output::Submission {
        use crate::client_commands::{CommandContext, CommandResult, TextCommandResult};
        use minimium_commands::operation::OperationId;
        let id = self.next_command_id;
        self.next_command_id = self.next_command_id.saturating_add(1);
        let catalog = crate::client_commands::catalog::catalog();
        let entry = catalog.resolve(&request.path);
        let identity = minimium_commands::results::OperationContext {
            request_id: id,
            correlation_nonce: None,
            operation: entry
                .map_or_else(|| OperationId(request.path.clone()), |e| e.spec.id.clone()),
            scope: entry.map_or("Client".into(), |e| e.spec.scope.into()),
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
        let outcome = (|| {
            let entry = entry.ok_or_else(|| {
                minimium_commands::CommandError::new(
                    minimium_commands::CommandErrorCode::UnknownCommand,
                    Some(&request.path),
                    "unknown operation",
                    None::<String>,
                )
            })?;
            if !entry.spec.scriptable {
                return Err(minimium_commands::CommandError::new(
                    minimium_commands::CommandErrorCode::Unavailable,
                    Some(&request.path),
                    "operation is not scriptable",
                    None::<String>,
                ));
            }
            let prepared = minimium_commands::syntax::bind::bind_values(
                catalog,
                &request.path,
                request.access,
                &request.arguments,
            )?;
            let command = crate::client_commands::bindings::lower(&prepared, entry)?;
            let mut context = CommandContext {
                portal_entity: None,
                fire_direction: [1.0, 0.0, 0.0],
                player_position: None,
            };
            Ok(
                match crate::client_commands::execute(command, &mut context) {
                    CommandResult::Action(action) => TextCommandResult::Action(action),
                    CommandResult::Rejected(error) => TextCommandResult::Rejected(error),
                },
            )
        })();
        (
            identity,
            outcome.unwrap_or_else(TextCommandResult::Rejected),
        )
    }
}
