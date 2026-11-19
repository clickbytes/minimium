use super::super::ServerRuntime;
use crate::{commands, server_command::ServerPropertyCommand};
use minimium_commands::{PropertySource, PropertyValue};
use std::time::Duration;

impl ServerRuntime {
    pub(super) fn apply_property_command(
        &mut self,
        command: &ServerPropertyCommand,
    ) -> commands::CommandReply {
        let report = |result: commands::PropertyReportOutcome| match result {
            commands::PropertyReportOutcome::Value(value) => {
                commands::CommandReply::success(commands::CommandPayload::Property(value))
            }
            commands::PropertyReportOutcome::Unknown(key) => {
                commands::CommandReply::success(commands::CommandPayload::UnknownProperty(key))
            }
            commands::PropertyReportOutcome::Unavailable { key, reason } => {
                commands::CommandReply::success(commands::CommandPayload::UnavailableProperty {
                    key,
                    reason,
                })
            }
        };
        match command {
            ServerPropertyCommand::Get(key) => report(self.property_report(key)),
            ServerPropertyCommand::SetAutosave(seconds) => {
                self.autosave_seconds = *seconds;
                self.scheduler.autosave_interval = Duration::from_secs(*seconds);
                self.scheduler.next_autosave = if *seconds == 0 {
                    None
                } else {
                    Some(self.now + self.scheduler.autosave_interval)
                };
                self.autosave_session_override = true;
                report(self.property_report("server.autosave_interval"))
            }
            ServerPropertyCommand::ResetAutosave => {
                let startup_value = self.startup_autosave_seconds;
                self.autosave_seconds = startup_value;
                self.scheduler.autosave_interval = Duration::from_secs(startup_value);
                self.scheduler.next_autosave =
                    (startup_value != 0).then_some(self.now + self.scheduler.autosave_interval);
                self.autosave_session_override = false;
                report(self.property_report("server.autosave_interval"))
            }
            ServerPropertyCommand::SetBolidInterval(interval) => {
                self.bolid_interval = *interval;
                self.bolid_session_override = true;
                self.schedule_next_bolid();
                report(self.property_report("server.sky.bolid_interval"))
            }
            ServerPropertyCommand::ResetBolidInterval => {
                self.bolid_interval = self.startup_bolid_interval;
                self.bolid_session_override = false;
                self.schedule_next_bolid();
                report(self.property_report("server.sky.bolid_interval"))
            }
            ServerPropertyCommand::List(prefix) => {
                let matching = self
                    .operation_catalog
                    .entries()
                    .filter_map(|entry| entry.property)
                    .filter(|spec| prefix.as_deref().is_none_or(|p| spec.key.starts_with(p)));
                let mut rows = Vec::with_capacity(128);
                let mut truncated = false;
                for spec in matching {
                    if rows.len() == 128 {
                        truncated = true;
                        break;
                    }
                    rows.push(*spec);
                }
                commands::CommandReply::success(commands::CommandPayload::PropertyList {
                    prefix: prefix.clone(),
                    truncated,
                    rows,
                })
            }
            ServerPropertyCommand::Help(target) => {
                if let Some(key) = target {
                    if let Some((entry, spec)) = self
                        .operation_catalog
                        .resolve(key)
                        .and_then(|entry| entry.property.map(|spec| (entry, spec)))
                        .filter(|(_, spec)| spec.key == key)
                    {
                        commands::CommandReply::success(commands::CommandPayload::PropertyHelp(
                            *spec,
                            entry.describe(),
                        ))
                    } else {
                        crate::server_command::catalog::command_help(&self.operation_catalog, key)
                            .map_or_else(
                                || {
                                    commands::CommandReply::error(
                                        "server.command.unknown",
                                        "unknown help target",
                                    )
                                },
                                |help| {
                                    commands::CommandReply::success(
                                        commands::CommandPayload::SingleCommandHelp(help),
                                    )
                                },
                            )
                    }
                } else {
                    commands::CommandReply::success(
                        commands::CommandPayload::PropertyCommandsHelp {
                            builtins: crate::server_command::catalog::builtin_usages(
                                &self.operation_catalog,
                            ),
                        },
                    )
                }
            }
        }
    }

    pub(super) fn property_report(&self, key: &str) -> commands::PropertyReportOutcome {
        let value = if key == "server.sky.bolid_interval" {
            let effective = self
                .bolid_interval
                .map_or_else(|| "off".to_owned(), |(a, b)| format!("{a}..{b}"));
            PropertyValue {
                requested: Some(effective.clone()),
                effective: Some(effective),
                default: Some("120..300".to_owned()),
                source: if self.bolid_session_override {
                    PropertySource::Session
                } else {
                    self.resolved_settings[key].source
                },
                difference_reason: None,
            }
        } else if matches!(
            key,
            "world.seed" | "world.generation_revision" | "world.generator_digest"
        ) {
            let manifest = self.game.worlds.primary().manifest;
            let actual = match key {
                "world.seed" => manifest.generation().seed().get().to_string(),
                "world.generation_revision" => manifest.generation().revision().get().to_string(),
                "world.generator_digest" => manifest
                    .generator_digest()
                    .as_bytes()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
                _ => unreachable!(),
            };
            let (requested, default, request_source) = if key == "world.seed" {
                crate::config::property_value(&self.resolved_settings, key)
                    .map(|(requested, default, source)| {
                        (requested.to_owned(), default.map(str::to_owned), source)
                    })
                    .unwrap_or_else(|| {
                        (
                            "auto".to_owned(),
                            Some("auto".to_owned()),
                            PropertySource::Default,
                        )
                    })
            } else {
                (actual.clone(), None, PropertySource::Runtime)
            };
            PropertyValue {
                requested: Some(requested),
                effective: Some(actual),
                default,
                source: PropertySource::Runtime,
                difference_reason: Some(format!(
                    "actual world manifest; requested source={}",
                    request_source.as_str()
                )),
            }
        } else if key.starts_with("world.time.") {
            let clock = match self.monotonic_ms(self.now) {
                Ok(clock) => clock,
                Err(error) => {
                    return commands::PropertyReportOutcome::Unavailable {
                        key: key.to_owned(),
                        reason: format!("runtime.monotonic_clock_failed: {error}"),
                    }
                }
            };
            let snapshot = match self.environment_clock.property_snapshot(clock) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    return commands::PropertyReportOutcome::Unavailable {
                        key: key.to_owned(),
                        reason: error.to_string(),
                    }
                }
            };
            let current = match key {
                "world.time.mode" => snapshot.mode.to_owned(),
                "world.time.scale" => crate::command_output::format_clock_scale(snapshot.rate_ppm),
                "world.time.offline_policy" => snapshot.offline_policy.to_owned(),
                "world.time.utc" => snapshot.utc_ms.to_string(),
                "world.time.revision" => snapshot.revision.to_string(),
                _ => return commands::PropertyReportOutcome::Unknown(key.to_owned()),
            };
            PropertyValue {
                requested: Some(current.clone()),
                effective: Some(current),
                default: None,
                source: PropertySource::Runtime,
                difference_reason: None,
            }
        } else {
            let Some((configured, default, configured_source)) =
                crate::config::property_value(&self.resolved_settings, key)
            else {
                return commands::PropertyReportOutcome::Unknown(key.to_owned());
            };
            let (requested, effective, source, difference_reason) = if key
                == "server.autosave_interval"
            {
                (
                    self.autosave_seconds.to_string(),
                    self.autosave_seconds.to_string(),
                    if self.autosave_session_override {
                        PropertySource::Session
                    } else {
                        configured_source
                    },
                    (self.autosave_seconds == 0).then_some("periodic saves disabled".to_owned()),
                )
            } else if key == "server.threads" {
                let effective = self.lanes.to_string();
                (
                    configured.to_owned(),
                    effective.clone(),
                    configured_source,
                    (configured != effective)
                        .then_some("auto resolved to the available worker lane count".to_owned()),
                )
            } else if key == "server.game_port" {
                let effective = self.endpoint.as_ref().map_or_else(
                    || "unavailable".to_owned(),
                    |endpoint| endpoint.game_port.to_string(),
                );
                (
                    configured.to_owned(),
                    effective.clone(),
                    configured_source,
                    (configured != effective)
                        .then_some("port 0 selected an available port".to_owned()),
                )
            } else {
                (
                    configured.to_owned(),
                    configured.to_owned(),
                    configured_source,
                    None,
                )
            };
            PropertyValue {
                requested: Some(requested),
                effective: Some(effective),
                default: default.map(str::to_owned),
                source,
                difference_reason,
            }
        };
        commands::PropertyReportOutcome::Value(commands::PropertyReport {
            key: key.to_owned(),
            value,
        })
    }
}
