//! Registers two neutral protected components and exercises host and module paths.
//! Compile only with `cargo check -p minimium-server-runtime --example owned_components`.
#[path = "support/owned_components_modules.rs"]
mod demo_modules;
#[path = "support/owned_components_host.rs"]
mod host;

use minimium_gameplay::{WorldEffect, WorldEffectOutcome};
use minimium_server_runtime::{ServerGameplayModule, *};
use minimium_world::entity::EntityId;
use std::{sync::Arc, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let alpha: Arc<ServerGameplayModule> = Arc::new(demo_modules::FirstOwnerModule);
    let beta: Arc<ServerGameplayModule> = Arc::new(demo_modules::SecondOwnerModule);
    let module_set = [alpha, beta];
    let (mut runtime, worlds, mut time) = host::new_runtime(&module_set)?;
    let [first_world, second_world] = worlds;

    let startup_receipts: Vec<_> = worlds
        .into_iter()
        .flat_map(|world| {
            runtime.take_startup_effect_receipts(
                "owned-component-alpha",
                "owned-component-spawn",
                world,
            )
        })
        .collect();
    for receipt in &startup_receipts {
        println!("startup receipt {:?}: {:?}", receipt.token, receipt.outcome);
    }
    let alpha_entity = spawned_entity(
        &startup_receipts,
        first_world,
        EntityId::new(1).expect("entity IDs are nonzero"),
    )?;
    let beta_entity = spawned_entity(
        &startup_receipts,
        first_world,
        EntityId::new(2).expect("entity IDs are nonzero"),
    )?;
    let other_world_entity = spawned_entity(
        &startup_receipts,
        second_world,
        EntityId::new(1).expect("entity IDs are nonzero"),
    )?;

    // The host reserves before adding protected projections. The opaque owner identity
    // comes from each module's descriptor registration, never from an effect field.
    let alpha_first = runtime.component_owner_port(demo_modules::FIRST_COMPONENT, first_world)?;
    let alpha_first_clone = alpha_first.clone();
    let alpha_second = runtime.component_owner_port(demo_modules::FIRST_COMPONENT, second_world)?;
    let beta_first = runtime.component_owner_port(demo_modules::SECOND_COMPONENT, first_world)?;
    runtime.reserve_component_entity(&alpha_first, alpha_entity)?;
    runtime.reserve_component_entity(&alpha_second, other_world_entity)?;
    runtime.reserve_component_entity(&beta_first, beta_entity)?;
    runtime.reserve_component_entity(&beta_first, alpha_entity)?;

    // Tick one attempts ordinary despawn while alpha's reservation is pending and
    // before either owner publishes a protected component projection.
    time.monotonic += Duration::from_millis(50);
    let _phase_zero = runtime.step(time)?;

    runtime.apply_owned_component_effects(
        &alpha_first,
        &[WorldEffect::SetComponent {
            world: first_world,
            entity: alpha_entity,
            component: demo_modules::component_value(demo_modules::FIRST_COMPONENT, 1),
        }],
    )?;
    runtime.apply_owned_component_effects(
        &beta_first,
        &[WorldEffect::SetComponent {
            world: first_world,
            entity: beta_entity,
            component: demo_modules::component_value(demo_modules::SECOND_COMPONENT, 2),
        }],
    )?;

    let wrong_owner = runtime.apply_owned_component_effects(
        &alpha_first,
        &[WorldEffect::SetComponent {
            world: first_world,
            entity: beta_entity,
            component: demo_modules::component_value(demo_modules::SECOND_COMPONENT, 3),
        }],
    );
    expect_owner_error(
        wrong_owner,
        ComponentOwnerError::Validation,
        "alpha port writing beta component",
    )?;

    let wrong_world = runtime.apply_owned_component_effects(
        &alpha_first,
        &[WorldEffect::SetComponent {
            world: second_world,
            entity: other_world_entity,
            component: demo_modules::component_value(demo_modules::FIRST_COMPONENT, 4),
        }],
    );
    expect_owner_error(
        wrong_world,
        ComponentOwnerError::Validation,
        "world-one alpha port writing world two",
    )?;

    // Tick two through five deliver each previous system receipt before emitting the
    // next operation; the last tick drains the successful General-component receipt.
    for _ in 0..4 {
        time.monotonic += Duration::from_millis(50);
        let _outcome = runtime.step(time)?;
    }

    runtime.release_component_entity(&alpha_first, alpha_entity)?;
    let alpha_despawn_while_beta_reserved = runtime.apply_owned_component_effects(
        &alpha_first,
        &[WorldEffect::DespawnEntity {
            world: first_world,
            entity: alpha_entity,
        }],
    );
    expect_owner_error(
        alpha_despawn_while_beta_reserved,
        ComponentOwnerError::Validation,
        "alpha despawn while beta still reserves the entity",
    )?;
    runtime.release_component_entity(&beta_first, alpha_entity)?;
    runtime.apply_owned_component_effects(
        &alpha_first,
        &[WorldEffect::DespawnEntity {
            world: first_world,
            entity: alpha_entity,
        }],
    )?;

    runtime.release_component_owner_port(&alpha_first)?;
    let stale_clone = runtime.reserve_component_entity(&alpha_first_clone, beta_entity);
    expect_owner_error(
        stale_clone,
        ComponentOwnerError::Revoked,
        "alpha port clone after release, targeting the existing beta entity",
    )?;
    runtime.release_component_entity(&alpha_second, other_world_entity)?;
    runtime.release_component_owner_port(&alpha_second)?;
    runtime.release_component_entity(&beta_first, beta_entity)?;
    runtime.release_component_owner_port(&beta_first)?;

    let shutdown = runtime.shutdown(time, std::time::Instant::now() + Duration::from_secs(2));
    shutdown.result?;
    Ok(())
}

fn spawned_entity(
    receipts: &[EffectReceipt],
    expected_world: minimium_world::world::WorldInstanceId,
    expected_entity: EntityId,
) -> Result<EntityId, Box<dyn std::error::Error>> {
    receipts
        .iter()
        .find_map(|receipt| match receipt.outcome {
            WorldEffectOutcome::EntitySpawned { world, entity }
                if world == expected_world && entity == expected_entity =>
            {
                Some(entity)
            }
            _ => None,
        })
        .ok_or_else(|| {
            format!("startup did not publish an empty entity in {expected_world:?}").into()
        })
}

fn expect_owner_error(
    result: Result<(), ComponentOwnerError>,
    expected: ComponentOwnerError,
    scenario: &str,
) -> Result<(), std::io::Error> {
    match result {
        Err(actual) if actual == expected => {
            println!("{scenario}: rejected as {actual:?}");
            Ok(())
        }
        Err(actual) => Err(std::io::Error::other(format!(
            "{scenario}: expected {expected:?}, got {actual:?}"
        ))),
        Ok(()) => Err(std::io::Error::other(format!(
            "{scenario}: expected rejection {expected:?}, operation succeeded"
        ))),
    }
}
