use super::super::Renderer;
use crate::{environment::EnvironmentFrameInput, session::SessionState};
use minimium_protocol::WorldContext;
use minimium_world::world::MembershipEpoch;

impl Renderer {
    /// Publish one immutable clock/context tuple for every consumer this frame.
    pub(in crate::renderer) fn update_environment_frame(&mut self) {
        if let Some(session) = self
            .application
            .connection
            .session()
            .filter(|session| session.state() == SessionState::Connected)
        {
            let frame = session.environment_frame();
            let context_matches = frame.context.is_some_and(|context| {
                Some(context) == self.application.game.session_context()
                    && context.instance() == self.application.game.current_instance
                    && self
                        .application
                        .game
                        .world_manifests
                        .get(&self.application.game.current_instance)
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

        if self.application.settings.demo {
            let manifest = self
                .application
                .game
                .world_manifests
                .get(&self.application.game.current_instance)
                .copied();
            self.environment_frame = match (self.demo_environment.as_ref(), manifest) {
                (Some(demo), Some(manifest)) => demo.frame(
                    manifest,
                    Some(WorldContext::new(
                        self.application.game.current_instance,
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
