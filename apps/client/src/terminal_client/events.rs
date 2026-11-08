use crate::messages::ClientText as Text;
use crate::session::SessionEvent;
use minimium_localization::{Locale, Localizer, UiMessage};
use minimium_protocol::ServerMessage;

pub(super) fn print_session_event(
    event: SessionEvent,
    show_snapshot: bool,
    localizer: &Localizer,
    locale: Locale,
) {
    match event {
        SessionEvent::PingRoundTrip(duration) => print(
            Text::Ping {
                milliseconds: duration.as_millis(),
            },
            localizer,
            locale,
        ),
        SessionEvent::Disconnected(reason) => print(Text::from(reason), localizer, locale),
        SessionEvent::Message(message) => match message {
            ServerMessage::Welcome { id, health, .. } => {
                print(Text::Connected { id, health }, localizer, locale)
            }
            ServerMessage::Error(code) => print(Text::ServerError { code }, localizer, locale),
            ServerMessage::Bye => print(Text::ServerClosed, localizer, locale),
            ServerMessage::AltitudeResult { position_mm, .. } => println!(
                "{}",
                localizer.format(locale, &UiMessage::AltitudeResult(position_mm.is_some()))
            ),
            ServerMessage::Pong => print(Text::Pong, localizer, locale),
            ServerMessage::Accepted => print(Text::Accepted, localizer, locale),
            ServerMessage::Chat {
                sender_id,
                sender_name,
                text,
            } => print(
                Text::Chat {
                    sender_name,
                    sender_id,
                    text,
                },
                localizer,
                locale,
            ),
            ServerMessage::HealthUpdate(health) => {
                print(Text::Health { health }, localizer, locale)
            }
            ServerMessage::ShotResult {
                target_id,
                remaining_health,
                killed,
            } => match (target_id, remaining_health, killed) {
                (Some(id), Some(health), true) => print(
                    Text::TargetRespawned {
                        target_id: id,
                        health,
                    },
                    localizer,
                    locale,
                ),
                (Some(id), Some(health), false) => print(
                    Text::Hit {
                        target_id: id,
                        health,
                    },
                    localizer,
                    locale,
                ),
                _ => print(Text::ShotMissed, localizer, locale),
            },
            ServerMessage::Snapshot {
                server_time_ms,
                full_snapshot,
                self_position,
                self_velocity,
                self_grounded,
                players,
                removed_players,
                ..
            } if show_snapshot => {
                print(
                    Text::TerminalSnapshot {
                        server_time_ms,
                        full_snapshot,
                        self_position,
                        self_velocity,
                        grounded: self_grounded,
                        player_updates: players.len(),
                        player_removals: removed_players.len(),
                    },
                    localizer,
                    locale,
                );
            }
            ServerMessage::Snapshot { .. }
            | ServerMessage::Bolid { .. }
            | ServerMessage::MovementPolicy(_)
            | ServerMessage::EnvironmentSync { .. }
            | ServerMessage::TimeProbeReply { .. }
            | ServerMessage::EntitySyncBegin { .. }
            | ServerMessage::EntitySyncRecord { .. }
            | ServerMessage::EntitySyncComplete { .. }
            | ServerMessage::MapTeleportResult { .. }
            | ServerMessage::ContentOffer { .. } => {}
            ServerMessage::TransitionPrepare { .. } => println!(
                "{}",
                localizer.format(locale, &UiMessage::WorldTransferPrepared)
            ),
            ServerMessage::TransitionCommitted { .. } => println!(
                "{}",
                localizer.format(locale, &UiMessage::WorldTransferCommitted)
            ),
            ServerMessage::TransitionCancelled { .. } => println!(
                "{}",
                localizer.format(locale, &UiMessage::WorldTransferCancelled)
            ),
        },
    }
}

fn print(message: Text, localizer: &Localizer, locale: Locale) {
    println!("{}", message.localized(localizer, locale));
}
