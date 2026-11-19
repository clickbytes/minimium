use super::super::{environment::EnvironmentViewKind, DesktopHost};
use crate::client_commands::{camera, output, FrontendAction};
use minimium_commands::results::OperationContext;

impl DesktopHost {
    pub(in crate::renderer) fn camera_operation(
        &mut self,
        identity: &OperationContext,
        action: &FrontendAction,
    ) -> Option<output::ClientResult> {
        let request = camera::Request::from_action(action)?;
        let view = &self.render_environment_frame.render_view;
        let eye_m = (self.render_environment_frame.context.is_some()
            || self.environment_frame.quality
                == crate::environment::SynchronizationQuality::LocalDemo)
            .then_some(view.camera_eye_world_m)
            .filter(|p| p.iter().all(|v| v.is_finite()));
        let sample_age_ms = self
            .metrics
            .last_redraw
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        let camera = camera::Camera {
            yaw_rad: &mut self.input.yaw,
            pitch_rad: &mut self.input.pitch,
            fov: &mut self.fov_degrees,
            eye_m,
            sample_age_ms,
            view: if eye_m.is_none() {
                "unavailable"
            } else if view.kind == EnvironmentViewKind::Overview {
                "overview_camera"
            } else {
                "presentation_camera"
            },
        };
        Some(camera::execute(
            identity.clone(),
            request,
            &mut self.application.settings.values,
            &self.application.settings.config_file_settings,
            Some(camera),
        ))
    }
}
