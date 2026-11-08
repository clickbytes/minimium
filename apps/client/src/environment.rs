//! Application-only demo inputs and local lakes.
#[cfg(feature = "desktop")]
pub(crate) mod demo;
#[cfg(feature = "desktop")]
pub(crate) mod lakes;

#[cfg(feature = "desktop")]
pub(crate) use demo::LocalDemoEnvironment;
#[cfg(feature = "desktop")]
pub(crate) use minimium_client_runtime::environment::diagnostics;
pub(crate) use minimium_client_runtime::environment::EnvironmentFrameInput;
#[cfg(feature = "desktop")]
pub(crate) use minimium_client_runtime::environment::SkyDiagnosticsSnapshot;
pub(crate) use minimium_client_runtime::environment::SynchronizationQuality;

pub(crate) fn localized_status_line(
    frame: EnvironmentFrameInput,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> String {
    let date = frame
        .render_world_utc_ms
        .filter(|_| frame.estimated_world_utc_ms.is_some() && frame.descriptor.is_some())
        .map(minimium_client_runtime::environment::format_utc)
        .unwrap_or_else(|| "UTC —".to_owned());
    let rate = frame.rate_ppm.map_or_else(
        || "—".to_owned(),
        |ppm| format!("{:.3}x", ppm as f64 / 1_000_000.0),
    );
    let revision = frame
        .revision
        .map_or_else(|| "—".to_owned(), |value| value.to_string());
    localizer.format(
        locale,
        &minimium_localization::UiMessage::EnvironmentStatus {
            quality: localized_quality(frame.quality, localizer, locale),
            date,
            rate,
            revision,
        },
    )
}

#[cfg(feature = "desktop")]
pub(crate) fn localized_title_suffix(
    frame: EnvironmentFrameInput,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> String {
    let date = frame
        .render_world_utc_ms
        .filter(|_| frame.estimated_world_utc_ms.is_some() && frame.descriptor.is_some())
        .map(minimium_client_runtime::environment::format_utc)
        .unwrap_or_else(|| "UTC --".to_owned());
    localizer.format(
        locale,
        &minimium_localization::UiMessage::EnvironmentTitle {
            quality: localized_quality(frame.quality, localizer, locale),
            date,
        },
    )
}

fn localized_quality(
    quality: SynchronizationQuality,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> String {
    let status = match quality {
        SynchronizationQuality::Initializing => {
            minimium_localization::UiStatus::EnvironmentInitializing
        }
        SynchronizationQuality::Synchronized => {
            minimium_localization::UiStatus::EnvironmentSynchronized
        }
        SynchronizationQuality::Stale => minimium_localization::UiStatus::EnvironmentStale,
        SynchronizationQuality::Unavailable => {
            minimium_localization::UiStatus::EnvironmentUnavailable
        }
        SynchronizationQuality::LocalDemo => minimium_localization::UiStatus::EnvironmentLocalDemo,
    };
    localizer.format(locale, &minimium_localization::UiMessage::Status(status))
}
