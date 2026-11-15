use super::*;

impl Renderer {
    pub(super) fn equip_tool(&mut self, slot: Option<u8>) {
        if let Some(session) = self.application.connection.session_mut() {
            if let Err(error) = session.equip_tool(slot) {
                self.fail_connection(format!("Could not equip tool: {error:?}"));
            }
        }
    }

    pub(super) fn queue_equipped_tool(&mut self) -> bool {
        if !self
            .application
            .connection
            .session()
            .is_some_and(|s| s.inventory().tool().is_some())
        {
            return false;
        }
        let direction = super::camera::camera_direction(
            self.application.game.gameplay.prediction.character,
            self.input.yaw,
            self.input.pitch,
        )
        .normalize()
        .to_array();
        if let Some(session) = self.application.connection.session_mut() {
            if let Err(error) = session.queue_tool_use(direction) {
                self.fail_connection(format!("Could not use tool: {error:?}"));
            }
        }
        true
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
            self.queue_equipped_tool();
        }
    }
}
