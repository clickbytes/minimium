use super::{ClientApplicationCore, PredictionPresentationRequest, PredictionSceneRequest};
use crate::world_readiness::CollisionSnapshot;

pub(super) fn prediction_request(
    application: &ClientApplicationCore,
    previous_position: [f32; 3],
    position: [f32; 3],
) -> Result<Option<PredictionSceneRequest<'_>>, String> {
    let Some(manifest) = application
        .game
        .world_manifests
        .get(&application.game.current_instance)
        .copied()
    else {
        return Ok(None);
    };
    let terrain_inputs = match application.scene_terrain_inputs() {
        Ok(Some(inputs)) => inputs,
        Ok(None) => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", error.code())),
    };
    Ok(Some(PredictionSceneRequest {
        context: application.game.session_context(),
        instance: application.game.current_instance,
        manifest,
        previous_position,
        position,
        terrain_inputs,
        world_catalog: &application.world_catalog,
    }))
}

pub(super) fn water_covers(application: &ClientApplicationCore, position: [f32; 3]) -> bool {
    let Some(manifest) = application
        .game
        .world_manifests
        .get(&application.game.current_instance)
        .copied()
    else {
        return false;
    };
    manifest.planet().is_none_or(|_| {
        let feet = application
            .game
            .gameplay
            .prediction
            .position_mm
            .map_or(position.map(f64::from), |position| {
                position.0.map(|value| value as f64 / 1000.0)
            });
        let up = minimium_world::world::character_frame(
            application.game.gameplay.prediction.character.gravity,
            position,
        )
        .0
        .map(f64::from);
        application
            .connection
            .session()
            .is_some_and(|session| session.water_region_covers_capsule(feet, up, 1.7))
    })
}

pub(super) fn scene_ready(
    application: &mut ClientApplicationCore,
    previous_position: [f32; 3],
    position: [f32; 3],
    can_fly: bool,
    readiness: &mut impl FnMut(PredictionSceneRequest<'_>) -> Result<Option<CollisionSnapshot>, String>,
    presentation: &mut Option<
        &mut dyn FnMut(&CollisionSnapshot, PredictionPresentationRequest) -> Result<bool, String>,
    >,
) -> Result<bool, String> {
    let Some(request) = prediction_request(application, previous_position, position)? else {
        return Ok(false);
    };
    let presentation_request = PredictionPresentationRequest {
        manifest: request.manifest,
        position: request.position,
        terrain_inputs: request.terrain_inputs.clone(),
    };
    let Some(snapshot) = readiness(request)? else {
        return Ok(false);
    };
    let session_ready = application
        .connection
        .session()
        .is_none_or(|session| session.impact_ready() && session.excavation_ready());
    if !snapshot.covers_position(position)
        || !water_covers(application, position)
        || !session_ready
        || !application.report_active_scene_ready_admitted(snapshot.clone())
    {
        return Ok(false);
    }
    if can_fly || presentation.is_none() {
        Ok(true)
    } else {
        presentation.as_deref_mut().expect("checked above")(&snapshot, presentation_request)
    }
}
