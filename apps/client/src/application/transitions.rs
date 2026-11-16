use minimium_protocol::WorldContext;
use minimium_world::world::{TransitionId, WorldInstanceId, WorldManifest};
use std::collections::HashMap;

// Overview zoom is camera distance in planet radii.
pub(crate) const MIN_OVERVIEW_ZOOM: f32 = 1.02;
pub(crate) const MAX_OVERVIEW_ZOOM: f32 = 8.0;

#[derive(Default)]
pub(crate) struct MapOverviewModel {
    instance: Option<WorldInstanceId>,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    dragging: bool,
    slider_dragging: bool,
    dragged: bool,
    drag_distance: f64,
}

impl MapOverviewModel {
    pub(crate) fn new(zoom: f32) -> Self {
        Self {
            zoom,
            ..Self::default()
        }
    }

    pub(crate) fn instance(&self) -> Option<WorldInstanceId> {
        self.instance
    }
    pub(crate) fn yaw(&self) -> f32 {
        self.yaw
    }
    pub(crate) fn pitch(&self) -> f32 {
        self.pitch
    }
    pub(crate) fn zoom(&self) -> f32 {
        self.zoom
    }
    pub(crate) fn is_dragging(&self) -> bool {
        self.dragging
    }
    pub(crate) fn is_slider_dragging(&self) -> bool {
        self.slider_dragging
    }
    pub(crate) fn was_dragged(&self) -> bool {
        self.dragged
    }

    pub(crate) fn set_zoom(&mut self, zoom: f32) {
        if zoom.is_finite() {
            self.zoom = zoom.clamp(MIN_OVERVIEW_ZOOM, MAX_OVERVIEW_ZOOM);
        }
    }

    pub(crate) fn begin_slider_drag(&mut self) {
        self.slider_dragging = true;
        self.dragged = true;
    }

    pub(crate) fn begin_drag(&mut self) {
        self.dragging = true;
        self.dragged = false;
        self.drag_distance = 0.0;
    }

    pub(crate) fn end_drag(&mut self) -> bool {
        self.dragging = false;
        std::mem::take(&mut self.slider_dragging)
    }

    pub(crate) fn drag_by(&mut self, dx: f64, dy: f64, horizontal: f32) {
        self.drag_distance += (dx * dx + dy * dy).sqrt();
        self.dragged |= self.drag_distance > 5.0;
        self.yaw += dx as f32 * 0.005 * horizontal;
        self.pitch = (self.pitch + dy as f32 * 0.005).clamp(-1.4, 1.4);
    }
    pub(crate) fn is_open(&self) -> bool {
        self.instance.is_some()
    }

    pub(crate) fn open(&mut self, instance: WorldInstanceId) {
        self.instance = Some(instance);
        self.dragging = false;
        self.slider_dragging = false;
        self.dragged = false;
        self.drag_distance = 0.0;
    }

    pub(crate) fn close(&mut self) {
        self.instance = None;
        self.dragging = false;
        self.slider_dragging = false;
        self.dragged = false;
        self.drag_distance = 0.0;
    }
}

pub(crate) struct WorldTransitionController {
    pending: HashMap<TransitionId, PendingSectorTransition>,
    map_teleport_armed: bool,
    presentation: TransferPresentationPhase,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum TransferPresentationPhase {
    #[default]
    Ready,
    Loading,
}

impl WorldTransitionController {
    pub(crate) fn new(teleport_loading: bool) -> Self {
        Self {
            pending: HashMap::new(),
            map_teleport_armed: false,
            presentation: if teleport_loading {
                TransferPresentationPhase::Loading
            } else {
                TransferPresentationPhase::Ready
            },
        }
    }

    pub(crate) fn register(
        &mut self,
        id: TransitionId,
        transition: PendingSectorTransition,
    ) -> bool {
        if let Some(current) = self.pending.get(&id) {
            return current.same_preparation(&transition);
        }
        if !self.pending.is_empty() {
            return false;
        }
        self.pending.insert(id, transition);
        true
    }

    pub(crate) fn pending_preparation(&self) -> Option<(TransitionId, &PendingSectorTransition)> {
        self.pending
            .iter()
            .next()
            .map(|(id, transition)| (*id, transition))
    }

    pub(crate) fn contains_id(&self, id: TransitionId) -> bool {
        self.pending.contains_key(&id)
    }

    pub(crate) fn commit(&mut self, id: TransitionId, context: WorldContext) -> bool {
        if !self
            .pending
            .get(&id)
            .is_some_and(|transition| transition.context == context)
        {
            return false;
        }
        self.pending.remove(&id);
        true
    }

    pub(crate) fn contains(&self, id: TransitionId, context: WorldContext) -> bool {
        self.pending
            .get(&id)
            .is_some_and(|transition| transition.context == context)
    }

    pub(crate) fn cancel(
        &mut self,
        id: TransitionId,
        source_context: WorldContext,
    ) -> Option<PendingSectorTransition> {
        if !self
            .pending
            .get(&id)
            .is_some_and(|t| t.source_context == source_context)
        {
            return None;
        }
        self.pending.remove(&id)
    }

    pub(crate) fn fail(&mut self, id: TransitionId) -> Option<PendingSectorTransition> {
        self.pending.remove(&id)
    }

    pub(crate) fn take_pending(&mut self) -> Vec<PendingSectorTransition> {
        std::mem::take(&mut self.pending).into_values().collect()
    }

    pub(crate) fn map_teleport_armed(&self) -> bool {
        self.map_teleport_armed
    }

    pub(crate) fn arm_map_teleport(&mut self) -> bool {
        if self.map_teleport_armed {
            return false;
        }
        self.map_teleport_armed = true;
        true
    }

    pub(crate) fn clear_map_teleport_arm(&mut self) {
        self.map_teleport_armed = false;
    }

    pub(crate) fn reset(&mut self) {
        self.map_teleport_armed = false;
        self.presentation = TransferPresentationPhase::Ready;
    }

    pub(crate) fn is_loading(&self) -> bool {
        self.presentation == TransferPresentationPhase::Loading
    }

    pub(crate) fn set_loading(&mut self, loading: bool) {
        self.presentation = if loading {
            TransferPresentationPhase::Loading
        } else {
            TransferPresentationPhase::Ready
        };
    }
}

#[derive(Clone)]
pub(crate) struct PendingSectorTransition {
    pub(crate) transition_id: TransitionId,
    pub(crate) source_context: WorldContext,
    pub(crate) context: WorldContext,
    pub(crate) destination: [f32; 3],
    pub(crate) manifest: WorldManifest,
    pub(crate) map: std::sync::Arc<minimium_world::world::WorldMap>,
}

impl PendingSectorTransition {
    pub(crate) fn preparing(
        transition_id: TransitionId,
        source_context: WorldContext,
        context: WorldContext,
        destination: [f32; 3],
        manifest: WorldManifest,
        map: std::sync::Arc<minimium_world::world::WorldMap>,
    ) -> Self {
        Self {
            transition_id,
            source_context,
            context,
            destination,
            manifest,
            map,
        }
    }

    fn same_preparation(&self, other: &Self) -> bool {
        self.transition_id == other.transition_id
            && self.source_context == other.source_context
            && self.context == other.context
            && self.destination == other.destination
            && self.manifest == other.manifest
            && self.map.fingerprint() == other.map.fingerprint()
    }
}
