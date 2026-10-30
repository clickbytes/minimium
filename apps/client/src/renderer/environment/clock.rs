use super::super::Renderer;
use crate::{environment::EnvironmentFrameInput, session::SessionState};
use minimium_core::world::MembershipEpoch;
use minimium_protocol::WorldContext;

impl Renderer {
    /// Publish one immutable clock/context tuple for every consumer this frame.
    pub(in crate::renderer) fn update_environment_frame(&mut self) {
        if let Some(session) = self
            .connection
            .session()
            .filter(|session| session.state() == SessionState::Connected)
        {
            let frame = session.environment_frame();
            let context_matches = frame.context.is_some_and(|context| {
                Some(context) == self.game.session_context
                    && context.instance() == self.game.current_instance
                    && self
                        .game
                        .world_manifests
                        .get(&self.game.current_instance)
                        .is_some_and(|manifest| {
                            context.content_revision() == manifest.content_revision()
                        })
            });
            self.environment_frame = if context_matches {
                frame
            } else {
                EnvironmentFrameInput::unavailable()
            };
            return;
        }

        if self.settings.demo {
            let manifest = self
                .game
                .world_manifests
                .get(&self.game.current_instance)
                .copied();
            self.environment_frame = match (self.demo_environment.as_ref(), manifest) {
                (Some(demo), Some(manifest)) => demo.frame(
                    manifest,
                    Some(WorldContext::new(
                        self.game.current_instance,
                        manifest.content_revision(),
                        MembershipEpoch::new(1).expect("local demo membership epoch is nonzero"),
                    )),
                ),
                _ => EnvironmentFrameInput::unavailable(),
            };
        } else {
            self.environment_frame = EnvironmentFrameInput::unavailable();
        }
    }
}
