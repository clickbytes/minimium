use super::{ServerCommand, ServerPropertyCommand};
use minimium_commands::operation::{OperationCatalog, OperationId};
use minimium_commands::results::OperationContext;
use minimium_environment::TimeCommand;

impl ServerCommand {
    pub(crate) fn identity(&self, request_id: u64, catalog: &OperationCatalog) -> OperationContext {
        let name = match self {
            Self::CatalogOutput { operation, .. } => operation,
            Self::Help => "help",
            Self::Status => "status",
            Self::Players { .. } => "players list",
            Self::Metrics => "metrics show",
            Self::Save => "world save",
            Self::Stop => "stop",
            Self::SpawnRandomBolid(_) => "spawn_random_bolid",
            Self::BolidStatus(_) => "bolids status",
            Self::ImpactReceiptStatus { .. } => "impacts status",
            Self::WaterStatus(_) => "water status",
            Self::SpawnRandomMeteorid => "spawn_random_meteorid",
            Self::TimeOfDay { solar: true, .. } => "world.time.set_solar_hhmm",
            Self::TimeOfDay { .. } => "world.time.set_utc_hhmm",
            Self::Time(command) => match command {
                TimeCommand::Status => "time status",
                TimeCommand::Mode(_) => "world.time.mode",
                TimeCommand::Set(_) => "time set",
                TimeCommand::Scale(_) => "world.time.scale",
                TimeCommand::Offline(_) => "world.time.offline_policy",
            },
            Self::Property(property) => match property {
                ServerPropertyCommand::Get(key) => key,
                ServerPropertyCommand::SetAutosave(_) | ServerPropertyCommand::ResetAutosave => {
                    "server.autosave_interval"
                }
                ServerPropertyCommand::SetBolidInterval(_)
                | ServerPropertyCommand::ResetBolidInterval => "server.sky.bolid_interval",
                ServerPropertyCommand::List(_) => "ui.console.list",
                ServerPropertyCommand::Help(_) => "help",
            },
            Self::Registered { identity, .. } => identity,
            Self::UnresolvedModule(words) => {
                return minimium_commands::syntax::bind::legacy_prefix(catalog, &words.join(" "))
                    .map(|(entry, _, _)| {
                        context(request_id, entry.spec.id.clone(), entry.spec.scope)
                    })
                    .unwrap_or_else(|| {
                        context(request_id, OperationId("ui.console.input".into()), "Server")
                    })
            }
        };
        let mut identity = catalog
            .resolve(name)
            .map(|entry| context(request_id, entry.spec.id.clone(), entry.spec.scope))
            .unwrap_or_else(|| context(request_id, OperationId(name.into()), "Server"));
        if let Self::TimeOfDay {
            observer: Some((_, context)),
            ..
        } = self
        {
            identity.context = Some(context.clone());
        }
        identity
    }
}

fn context(request_id: u64, operation: OperationId, scope: &str) -> OperationContext {
    OperationContext {
        request_id,
        correlation_nonce: None,
        operation,
        scope: scope.to_owned(),
        session: None,
        context: None,
    }
}
