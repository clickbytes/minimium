use super::*;

impl DesktopHost {
    pub(super) fn equip_tool(&mut self, slot: Option<u8>) {
        if let crate::application::LocalActionOutcome::Rejected { error, .. } =
            self.application.execute_action(
                crate::application::ClientAction::EquipTool { slot },
                crate::host_time::sample(),
            )
        {
            if error != crate::session::QueueError::Disconnected {
                self.fail_connection(format!("Could not equip tool: {error:?}"));
            }
        }
    }

    pub(super) fn queue_equipped_use(&mut self, trigger: crate::application::UseTrigger) {
        let view = self.application.core_view();
        let character = view.character;
        let feet_m = view.position_mm.map_or_else(
            || character.position.map(f64::from),
            |position| position.0.map(|value| value as f64 / 1000.0),
        );
        let Some(geometry) = CharacterGeometry::new(feet_m, character.gravity) else {
            return;
        };
        let direction = super::camera::camera_direction_from_geometry(
            geometry,
            self.input.yaw,
            self.input.pitch,
        )
        .normalize()
        .to_array();
        match self.application.execute_action(
            crate::application::ClientAction::UseEquipped { direction, trigger },
            crate::host_time::sample(),
        ) {
            crate::application::LocalActionOutcome::Queued {
                kind: crate::application::QueuedActionKind::ToolUse,
                ..
            } => self.start_tool_swing(),
            crate::application::LocalActionOutcome::Rejected { error, .. } => {
                self.fail_connection(format!("Could not use tool: {error:?}"));
            }
            _ => {}
        }
    }

    pub(super) fn update_tool_use(&mut self) {
        if self.input.cursor_locked
            && !self.application.ui.console.open
            && !self.application.overview().is_open()
            && self
                .application
                .input
                .is_active(crate::input::InputAction::Fire)
        {
            self.queue_equipped_use(crate::application::UseTrigger::Held);
        }
    }
}
