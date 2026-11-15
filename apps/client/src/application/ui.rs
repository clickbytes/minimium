#[cfg(feature = "desktop")]
pub(crate) struct UiModel {
    pub(crate) console: crate::console::Console,
    pub(crate) session_events: Vec<minimium_client_runtime::SessionEvent>,
}

#[derive(Clone, Debug)]
pub(crate) enum ClientStatus {
    Status(minimium_localization::UiStatus),
    Message(minimium_localization::UiMessage),
    ClientText(crate::messages::ClientText),
}

impl ClientStatus {
    pub(crate) fn format(
        &self,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) -> String {
        match self {
            Self::Status(status) => {
                localizer.format(locale, &minimium_localization::UiMessage::Status(*status))
            }
            Self::Message(message) => localizer.format(locale, message),
            Self::ClientText(message) => message.localized(localizer, locale),
        }
    }

    pub(crate) fn is_error(&self) -> bool {
        matches!(
            self,
            Self::Message(minimium_localization::UiMessage::EarthError(_)) | Self::ClientText(_)
        )
    }
}
