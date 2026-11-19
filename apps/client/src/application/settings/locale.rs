use super::SettingsStore;
use crate::client_commands::{
    property_spec, ClientPropertyCommand as Cmd, ClientPropertyKey as Key,
    ClientPropertyValue as Value, FrontendAction,
};
use minimium_commands::operation::CommandValue;
use minimium_commands::results::{
    property::{Freshness, ReadSource, StartupValue},
    OperationContext, OperationFailure, OperationReceipt, OperationResult, Payload, Stage,
};
use minimium_commands::{CommandError, CommandErrorCode, PropertySource, PropertyValue};
use minimium_localization::Locale;

impl SettingsStore {
    /// Apply the locale operation once. The host receives the effective locale for UI refresh.
    pub(crate) fn locale_operation(
        &mut self,
        identity: &OperationContext,
        action: &FrontendAction,
    ) -> Option<(OperationResult, Option<Locale>)> {
        let FrontendAction::Property(command) = action else {
            return None;
        };
        let key = Key::Locale.key();
        let change = match command {
            Cmd::Get(Key::Locale) => None,
            Cmd::Set(Value::Locale(locale)) => Some(Some(*locale)),
            Cmd::Reset(Key::Locale) => Some(None),
            _ => return None,
        };
        let stage = if change.is_some() {
            Stage::OwnerCommitted
        } else {
            Stage::Completed
        };
        if let Some(value) = change {
            let Some(revision) = self.locale_revision.checked_add(1) else {
                let error = CommandError::new(
                    CommandErrorCode::Conflict,
                    Some(key),
                    "setting revision exhausted",
                    None::<String>,
                );
                return Some((
                    OperationResult::new(
                        identity.clone(),
                        OperationReceipt::at(Stage::Rejected),
                        Err(OperationFailure::command("SettingsStore", error)),
                    ),
                    None,
                ));
            };
            if let Some(locale) = value {
                self.set(Value::Locale(locale));
            } else {
                self.reset(key).expect("validated locale");
            }
            self.locale_revision = revision;
        }
        let record = &self.records[key];
        let Value::Locale(locale) = record.requested.typed else {
            unreachable!("validated locale")
        };
        let value = CommandValue::Enum(record.requested.encoded.clone());
        let startup = self
            .startup_projection(key)
            .expect("resolved startup locale");
        let property = PropertyValue {
            lifecycle: minimium_commands::PropertyLifecycle::Live,
            requested: Some(value.clone()),
            effective: Some(value.clone()),
            default: property_spec(key)
                .and_then(|s| s.default)
                .map(|v| CommandValue::Enum(v.to_owned())),
            startup: Some(StartupValue {
                value: CommandValue::Enum(startup.value),
                source: startup.source,
            }),
            session_override: (record.source == PropertySource::Session).then_some(value),
            source: record.source,
            read_source: ReadSource::LocalOwner,
            revision: Some(self.locale_revision),
            freshness: Freshness::Current,
            difference_reason: None,
        };
        let mut receipt = OperationReceipt::at(stage);
        receipt.revision = property.revision;
        receipt.freshness = Some(property.freshness);
        Some((
            OperationResult::new(identity.clone(), receipt, Ok(Payload::Property(property))),
            change.map(|_| locale),
        ))
    }
}
