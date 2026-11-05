//! Immutable CPU-owned presentation data for one frame.
use crate::{
    gpu_frame::{CameraUniform, EnvironmentUniform},
    RenderViewFrame, UiVertex,
};
use minimium_world::world::WorldInstanceId;

/// GPU-independent client world context adapted from `ClientView` for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClientFrameState {
    pub world_instance: WorldInstanceId,
    pub player_position_m: [f64; 3],
    pub health: u16,
    pub sectors_ready: bool,
}

/// Prepared camera, environment and overlay values consumed by GPU submission.
/// The frame owns CPU values only and never carries GPU resource handles.
pub struct RenderFrame {
    pub view: RenderViewFrame,
    pub camera: CameraUniform,
    pub environment: EnvironmentUniform,
    pub halo_count: u32,
    /// Client presentation context attached by the application adapter.
    pub client: Option<ClientFrameState>,
    ui_vertices: Vec<UiVertex>,
}

impl RenderFrame {
    /// Creates a frame with prepared camera/environment values and no overlay geometry.
    pub fn new(
        view: RenderViewFrame,
        camera: CameraUniform,
        environment: EnvironmentUniform,
        halo_count: u32,
    ) -> Self {
        Self {
            view,
            camera,
            environment,
            halo_count,
            client: None,
            ui_vertices: Vec::new(),
        }
    }

    /// Attaches the immutable client state mapped from `ClientView` by the host.
    pub fn with_client_state(mut self, client: ClientFrameState) -> Self {
        self.client = Some(client);
        self
    }

    /// Returns a new frame carrying the overlay vertices prepared by the UI adapter.
    pub fn with_ui_vertices(mut self, vertices: Vec<UiVertex>) -> Self {
        self.ui_vertices = vertices;
        self
    }

    /// Returns overlay storage to the application for reuse after frame submission.
    pub fn take_ui_vertices(&mut self) -> Vec<UiVertex> {
        std::mem::take(&mut self.ui_vertices)
    }

    /// Uploads this frame's bounded overlay batch and returns its vertex count.
    pub fn upload_ui(&self, queue: &wgpu::Queue, buffer: &wgpu::Buffer) -> u32 {
        if self.ui_vertices.is_empty() {
            return 0;
        }
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&self.ui_vertices));
        self.ui_vertices.len() as u32
    }

    /// Uploads this frame's camera, environment and overlay values to caller-owned GPU buffers.
    pub fn upload(
        &self,
        queue: &wgpu::Queue,
        camera_buffer: &wgpu::Buffer,
        environment_buffer: &wgpu::Buffer,
        ui_buffer: &wgpu::Buffer,
        hdr_enabled: bool,
        output: Option<&crate::OutputEncoder>,
    ) -> u32 {
        queue.write_buffer(camera_buffer, 0, bytemuck::bytes_of(&self.camera));
        let mut environment = self.environment;
        environment.flags[3] = u32::from(hdr_enabled);
        queue.write_buffer(environment_buffer, 0, bytemuck::bytes_of(&environment));
        if let Some(output) = output {
            output.update_exposure(queue, environment.ambient_rgb_exposure[3]);
        }
        self.upload_ui(queue, ui_buffer)
    }
}
