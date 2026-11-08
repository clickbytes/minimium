use fluent_bundle::{FluentBundle, FluentResource};
use minimium_diagnostics::{DomainLogger, EventCode, Level};
use std::collections::BTreeMap;
use unic_langid::LanguageIdentifier;

mod messages;
pub use messages::UiMessage;

mod catalogs {
    pub const EN: &str = include_str!("../locales/en.ftl");
    pub const PL: &str = include_str!("../locales/pl.ftl");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Locale {
    English,
    Polish,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueAction {
    Command,
    Fire,
    Movement,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueCause {
    Disconnected,
    AwaitingWelcome,
    Authentication,
    StaleWorld,
    Full,
    Sequence,
    Protocol,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerErrorReason {
    Protocol,
    Busy,
    Full,
    RateLimit,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiStatus {
    EarthConnecting,
    EarthDisconnected,
    EarthConnected,
    Connected,
    EarthLoading,
    EarthChooseLand,
    EarthClickLand,
    EarthClickSurface,
    EarthMoving,
    EarthMoved,
    TerrainMaterials,
    ConnectionTimedOut,
    EarthServerClosed,
    ContentUnavailable,
    EnvironmentInitializing,
    EnvironmentSynchronized,
    EnvironmentStale,
    EnvironmentUnavailable,
    EnvironmentLocalDemo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiNotice {
    ConsoleWelcome,
    ConsoleOverview,
    CommandsHeader,
    TimeRequestSent,
    TimeScaleRequestSent,
    BolidRequestSent,
    BolidRequestQueued,
    TimeModeRequestSent,
    TimeOfflinePolicyRequestSent,
    SkyBodiesUnavailable,
    MapTeleportDesktopOnly,
    SkyInfoDesktopOnly,
    WeatherRequiresDesktop,
    MeteorsRequireDesktop,
    SkyQualityDesktopOnly,
    MapTeleportBusy,
    MapTeleportRequiresEarth,
    LocalServerThreadsConflict,
    StarCatalogUnavailable,
    TimeRequestFailed,
    TimeScaleRequestFailed,
    TimeModeRequestFailed,
    TimeOfflinePolicyRequestFailed,
    TimeScaleResponseFailed,
    EnvironmentUnavailable,
    ClipboardUnavailable,
    AltitudeRequiresFlight,
    PortalSnapshotUnavailable,
    PortalUnavailable,
    LocalServerConfigFailed,
}

impl Locale {
    pub fn from_setting(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "en" => Some(Self::English),
            "pl" => Some(Self::Polish),
            _ => None,
        }
    }
}

type Bundle = FluentBundle<FluentResource>;

pub struct Localizer {
    bundles: BTreeMap<LocaleKey, Bundle>,
    logger: Option<DomainLogger>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum LocaleKey {
    En,
    Pl,
}

impl Localizer {
    pub fn new(logger: Option<DomainLogger>) -> Result<Self, String> {
        let en = make_bundle("en", catalogs::EN)?;
        let pl = make_bundle("pl", catalogs::PL)?;
        Ok(Self {
            bundles: [(LocaleKey::En, en), (LocaleKey::Pl, pl)].into(),
            logger,
        })
    }

    pub fn format(&self, locale: Locale, message: &UiMessage) -> String {
        let key = message.key();
        let args = message.args();
        let preferred = if locale == Locale::Polish {
            LocaleKey::Pl
        } else {
            LocaleKey::En
        };
        for bundle_key in
            std::iter::once(preferred).chain((preferred == LocaleKey::Pl).then_some(LocaleKey::En))
        {
            let Some(bundle) = self.bundles.get(&bundle_key) else {
                continue;
            };
            let Some(pattern) = bundle.get_message(key).and_then(|entry| entry.value()) else {
                continue;
            };
            let mut errors = Vec::new();
            let result = bundle
                .format_pattern(pattern, Some(&args), &mut errors)
                .into_owned();
            if !errors.is_empty() {
                self.report(key, "catalog_format_error");
                continue;
            }
            return result;
        }
        self.report(key, "catalog_key_missing");
        key.to_owned()
    }

    fn report(&self, key: &str, code: &'static str) {
        if let Some(logger) = &self.logger {
            if let Ok(event_code) = EventCode::new(code) {
                logger.emit(logger.event(Level::Warn, event_code).field("key", key));
            }
        }
    }
}

fn make_bundle(locale: &str, source: &str) -> Result<Bundle, String> {
    let language: LanguageIdentifier = locale
        .parse()
        .map_err(|error| format!("invalid locale: {error}"))?;
    let mut bundle = FluentBundle::new(vec![language]);
    // Both shipped catalogs are LTR. Fluent's bidi isolation markers have no
    // visual role here and the fixed UI atlas cannot render them as glyphs.
    bundle.set_use_isolating(false);
    let resource = FluentResource::try_new(source.to_owned())
        .map_err(|(_, errors)| format!("invalid Fluent catalog: {errors:?}"))?;
    bundle
        .add_resource(resource)
        .map_err(|errors| format!("duplicate Fluent catalog keys: {errors:?}"))?;
    Ok(bundle)
}
