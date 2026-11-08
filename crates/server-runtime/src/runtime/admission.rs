mod actions;
mod dispatch;
mod identity;
mod queue;
mod routing;
mod technical;
mod world_control;

use super::ServerRuntime;

impl ServerRuntime {
    pub(super) fn admit_messages(&mut self) {
        let mut messages = std::mem::take(&mut self.connections.messages);
        for (index, message, decoded_at) in messages.drain(..) {
            if index >= self.connections.clients.len()
                || self.connections.disconnected.contains(&index)
                || self.connections.clients[index].close_deadline().is_some()
            {
                continue;
            }
            dispatch::dispatch_message(self, index, message, decoded_at);
        }
        identity::poll_authentication(self);
        self.connections.messages = messages;
    }
}
