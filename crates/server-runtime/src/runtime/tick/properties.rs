use super::super::ServerRuntime;
use crate::{commands, server_command::ServerPropertyCommand};
use minimium_commands::{PropertySource, PropertyValue};
use std::time::{Duration, Instant};

impl ServerRuntime {
    pub(super) fn apply_property_command(
        &mut self,
        command: &ServerPropertyCommand,
    ) -> commands::CommandReply {
        let reply = |success, text: String| commands::CommandReply {
            success,
            text,
            stop: false,
        };
        match command {
            ServerPropertyCommand::Get(key) => reply(true, self.property_report(key)),
            ServerPropertyCommand::SetAutosave(seconds) => {
                self.autosave_seconds = *seconds;
                self.scheduler.autosave_interval = Duration::from_secs(*seconds);
                self.scheduler.next_autosave = if *seconds == 0 {
                    None
                } else {
                    Some(Instant::now() + self.scheduler.autosave_interval)
                };
                self.autosave_session_override = true;
                reply(true, self.property_report("server.autosave_interval"))
            }
            ServerPropertyCommand::ResetAutosave => {
                let startup_value = self.resolved_settings["server.autosave_interval"]
                    .requested
                    .parse::<u64>()
                    .expect("startup autosave interval was validated");
                self.autosave_seconds = startup_value;
                self.scheduler.autosave_interval = Duration::from_secs(startup_value);
                self.scheduler.next_autosave = (startup_value != 0)
                    .then_some(Instant::now() + self.scheduler.autosave_interval);
                self.autosave_session_override = false;
                reply(true, self.property_report("server.autosave_interval"))
            }
            ServerPropertyCommand::List(prefix) => {
                let lines = crate::server_command::server_properties()
                    .filter(|spec| prefix.as_deref().is_none_or(|p| spec.key.starts_with(p)))
                    .map(|spec| {
                        format!(
                            "{}: {}",
                            spec.key,
                            spec.constraints.unwrap_or("no constraints")
                        )
                    })
                    .collect::<Vec<_>>();
                if lines.is_empty() {
                    reply(
                        true,
                        format!(
                            "no server properties match {}",
                            prefix.as_deref().unwrap_or("the requested prefix")
                        ),
                    )
                } else {
                    reply(true, lines.join("\n"))
                }
            }
            ServerPropertyCommand::Help(target) => {
                if let Some(key) = target {
                    if let Some(spec) =
                        crate::server_command::server_properties().find(|spec| spec.key == key)
                    {
                        reply(true, format!("{}: {:?}; default {}; constraints {}; owner {}; permission {:?}; lifecycle {:?}; persistent {}; resettable {}\n{}", spec.key, spec.value_type, spec.default.unwrap_or("n/a"), spec.constraints.unwrap_or("none"), spec.owner, spec.permission, spec.lifecycle, spec.persistent, spec.resettable, spec.description))
                    } else {
                        reply(
                            true,
                            crate::server_command::server_command_usage(key)
                                .expect("help target was validated")
                                .to_owned(),
                        )
                    }
                } else {
                    reply(
                        true,
                        format!(
                            "{}\nget KEY | set KEY VALUE | reset KEY | list [PREFIX]",
                            crate::server_command::server_help()
                        ),
                    )
                }
            }
        }
    }

    pub(super) fn property_report(&self, key: &str) -> String {
        if matches!(
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
                _ => unreachable!("world identity key was matched"),
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
            let value = PropertyValue {
                requested: Some(requested),
                effective: Some(actual),
                default,
                source: PropertySource::Runtime,
                difference_reason: Some(format!(
                    "actual world manifest; requested source={}",
                    request_source.as_str()
                )),
            };
            return format!(
                "{key} requested={} effective={} default={} source={} reason={}",
                value.requested.as_deref().unwrap_or("unavailable"),
                value.effective.as_deref().unwrap_or("unavailable"),
                value.default.as_deref().unwrap_or("n/a"),
                value.source.as_str(),
                value.difference_reason.as_deref().unwrap_or("none")
            );
        }
        if key.starts_with("world.time.") {
            let value = self
                .monotonic_ms(Instant::now())
                .map_err(|error| error.to_string())
                .and_then(|now| self.environment_clock.property_snapshot(now))
                .ok();
            let Some(clock) = value else {
                return format!("{key} requested=unavailable effective=unavailable default=n/a source=unavailable reason=clock sample unavailable");
            };
            let current = match key {
                "world.time.mode" => clock.mode.to_owned(),
                "world.time.scale" => clock.scale,
                "world.time.offline_policy" => clock.offline_policy.to_owned(),
                "world.time.utc" => clock.utc_ms.to_string(),
                "world.time.revision" => clock.revision.to_string(),
                _ => return format!("unknown server property `{key}`"),
            };
            let value = PropertyValue {
                requested: Some(current.clone()),
                effective: Some(current),
                default: None,
                source: PropertySource::Runtime,
                difference_reason: None,
            };
            return format!(
                "{key} requested={} effective={} default=n/a source={} reason=none",
                value.requested.as_deref().unwrap_or("unavailable"),
                value.effective.as_deref().unwrap_or("unavailable"),
                value.source.as_str()
            );
        }
        let Some((configured, default, configured_source)) =
            crate::config::property_value(&self.resolved_settings, key)
        else {
            return format!("unknown server property `{key}`");
        };
        let (requested, effective, source, difference_reason) = if key == "server.autosave_interval"
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
            let effective = self.endpoint.game_port.to_string();
            (
                configured.to_owned(),
                effective.clone(),
                configured_source,
                (configured != effective).then_some("port 0 selected an available port".to_owned()),
            )
        } else {
            (
                configured.to_owned(),
                configured.to_owned(),
                configured_source,
                None,
            )
        };
        let value = PropertyValue {
            requested: Some(requested),
            effective: Some(effective),
            default: default.map(str::to_owned),
            source,
            difference_reason,
        };
        format!(
            "{key} requested={} effective={} default={} source={} reason={}",
            value.requested.as_deref().unwrap_or("unavailable"),
            value.effective.as_deref().unwrap_or("unavailable"),
            value.default.as_deref().unwrap_or("n/a"),
            value.source.as_str(),
            value.difference_reason.as_deref().unwrap_or("none")
        )
    }
}
