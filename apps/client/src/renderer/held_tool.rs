//! Desktop-owned playback of the renderer's tool-independent digging pose.
use super::*;
use minimium_renderer::held_tool::{HeldToolPose, DIG_SWING_DURATION};

type Scope = (minimium_protocol::WorldContext, usize);

#[derive(Default)]
pub(super) struct HeldToolMotion {
    scope: Option<Scope>,
    started: Option<Duration>,
}

impl HeldToolMotion {
    pub(super) fn clear(&mut self) {
        self.scope = None;
        self.started = None;
    }

    fn start(&mut self, scope: Scope, now: Duration) {
        self.scope = Some(scope);
        self.started = Some(now);
    }

    pub(super) fn pose(
        &mut self,
        scope: Option<Scope>,
        controlled: bool,
        now: Duration,
    ) -> HeldToolPose {
        if !controlled || scope != self.scope {
            self.clear();
        }
        let Some(started) = self.started else {
            return HeldToolPose::default();
        };
        let elapsed = now.saturating_sub(started);
        if elapsed >= DIG_SWING_DURATION {
            self.started = None;
        }
        HeldToolPose::digging(elapsed)
    }
}

impl DesktopHost {
    pub(super) fn cancel_tool_swing(&mut self) {
        self.held_tool_motion.clear();
        self.renderer.held_tool_pose = HeldToolPose::default();
    }

    pub(super) fn start_tool_swing(&mut self) {
        if self.environment_frame.rate_ppm == Some(0)
            || !self.input.cursor_locked
            || self.application.ui.console.open
            || self.application.overview().is_open()
        {
            return;
        }
        let view = self.application.desktop_render_view();
        if let Some(scope) = view.context.zip(view.equipped_tool_model_index) {
            // One local attempt, no queue of delayed swings and no success claim.
            self.held_tool_motion.start(scope, self.presentation_time());
        }
    }
}
